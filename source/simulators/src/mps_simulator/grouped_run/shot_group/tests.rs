// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot group's rules: a shot's uniform reads One exactly when it is
//! below the probability of One ([`reads_one`]), so a uniform of 0 reads
//! One wherever One has a probability, however small, and Zero wherever
//! it has none; a shot draws its next uniform at every draw, even where
//! the value is certain; the shots that go on from a split are the fewer
//! value's, a tie going to Zero; and a split keeps each group's shots in
//! the run's order. That a run so keeps few states at once is in the tests
//! beside `grouped_run.rs`.

use crate::mps_simulator::grouped_run::shot_group::{Shot, reads_one, value_going_on};
use crate::mps_simulator::grouped_run::test_utils::{in_a_run, the_first_group};
use crate::mps_simulator::shot_rng::shot_rng;
use crate::mps_simulator::test_utils::ids;
use crate::mps_simulator::test_utils::programs::{program, ret};
use crate::mps_simulator::test_utils::reference_spy::ReferenceSpy;
use crate::qir::QirShotId;

/// A shot's uniform, in [0, 1), reads One exactly when it is below the
/// probability of One. So the smallest uniform, 0, reads One wherever One
/// has a probability, however small (about 10⁻¹⁴ for a qubit turned by
/// Ry(2·10⁻⁷)), and Zero where it has none; the largest, just below 1,
/// reads One where One is certain.
#[test]
fn a_uniform_reads_one_exactly_when_it_is_below_the_probability_of_one() {
    // The largest uniform a shot draws, 1 - 2⁻⁵³.
    let largest = 1.0_f64.next_down();
    // Each row: the scene, the uniform, P(One), and whether it reads One.
    let rows = [
        ("0 where One has no probability", 0.0, 0.0, false),
        ("0 where One has a tiny probability", 0.0, 1e-14, true),
        ("0 where One is certain", 0.0, 1.0, true),
        ("the largest where One is certain", largest, 1.0, true),
        ("a uniform equal to P(One)", 0.5, 0.5, false),
        ("a uniform above P(One)", 0.75, 0.5, false),
    ];
    for (scene, uniform, probability_of_one, one) in rows {
        assert_eq!(reads_one(uniform, probability_of_one), one, "{scene}");
    }
}

/// A shot draws its next uniform at every draw, even where P(One) is 0 or
/// 1 and the value is certain, so its later draws are those it would make
/// were the value uncertain; each shot draws from its own RNG, in the
/// group's order. Shots 0, 1 and 3 draw at P(One) 0, then 1, then ½: the
/// third draw reads each shot's third uniform, about 0.997, 0.09 and 0.343
/// at the seed `in_a_run` runs at, 7.
#[test]
fn a_shot_draws_its_next_uniform_even_where_the_value_is_certain() {
    let engine = ReferenceSpy::new();
    let shots = ids(&[0, 1, 3]);
    let draws = in_a_run(
        &engine,
        &program(1, &[&[ret()]]),
        &shots,
        None,
        |mut run| {
            let mut group = the_first_group(&mut run);
            [0.0, 1.0, 0.5].map(|probability_of_one| group.draw_each_shot(probability_of_one))
        },
    );
    let third_reads_one: Vec<bool> = shots
        .iter()
        .map(|&shot| {
            let mut rng = shot_rng(7, shot);
            let _certain = [rng.next_uniform(), rng.next_uniform()];
            rng.next_uniform() < 0.5
        })
        .collect();
    assert_eq!(
        third_reads_one,
        [false, true, true],
        "the shots' third draws"
    );
    assert_eq!(draws, [vec![false; 3], vec![true; 3], third_reads_one]);
}

/// The shots that go on from a split are the fewer value's, so a group
/// that splits keeps at most half its shots; a tie goes to Zero; and shots
/// that all drew one value go on with it. Each row is the shots' draws
/// (One when true), and the value going on.
#[test]
fn the_fewer_value_goes_on_and_a_tie_goes_to_zero() {
    let rows = [
        ("One the fewer", vec![false, true, false], true),
        ("Zero the fewer", vec![true, false, true], false),
        ("a tie", vec![true, false, false, true], false),
        ("all One", vec![true, true], true),
        ("all Zero", vec![false, false], false),
    ];
    for (scene, draws, one) in rows {
        assert_eq!(value_going_on(&draws), one, "{scene}");
    }
}

/// A split keeps each group's shots in the run's order, by which the run
/// tells which groups come after its failure: of five shots that drew One,
/// Zero, One, Zero, Zero, the group keeps shots 0 and 2, and gives 1, 3
/// and 4.
#[test]
fn a_split_keeps_each_groups_shots_in_order() {
    let engine = ReferenceSpy::new();
    let program = program(1, &[&[ret()]]);
    let places = in_a_run(&engine, &program, &QirShotId::first(5), None, |mut run| {
        let mut group = the_first_group(&mut run);
        let given = group.keep_shots_that_drew(&[true, false, true, false, false], true);
        let places = |shots: &[_]| {
            shots
                .iter()
                .map(|shot: &Shot| shot.place)
                .collect::<Vec<_>>()
        };
        (places(&group.shots), places(&given))
    });
    assert_eq!(places, (vec![0, 2], vec![1, 3, 4]));
}
