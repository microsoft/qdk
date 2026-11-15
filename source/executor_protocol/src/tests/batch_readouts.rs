// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A batch's readouts hold a row of exactly R values per shot, in push
//! order, with each shot's conditions; `concat_readouts` joins the parts of
//! one batch; conditions merge by union, and no operation clears one.

use super::allocations;
use crate::{
    BatchReadouts, ReadoutFailure, ReadoutIndex, ReadoutValue, Readouts, ShapeFault, ShotConditions,
};

use ReadoutValue::{One, Zero};

/// A lost value.
const LOST: ReadoutValue = ReadoutValue::Missing(ReadoutFailure::Loss);

/// Readouts of `width` slots, holding `rows` with their conditions.
fn readouts(width: u32, rows: &[(&[ReadoutValue], ShotConditions)]) -> BatchReadouts {
    let mut batch = BatchReadouts::with_capacity(rows.len(), width);
    for (values, conditions) in rows {
        batch
            .push_row(values, *conditions)
            .expect("each row has the batch's width");
    }
    batch
}

/// An executor that returns a row of the wrong width is refused at once,
/// naming both widths, and the readouts keep only their earlier rows.
#[test]
fn a_row_of_another_width_is_refused_and_not_added() {
    let mut batch = readouts(2, &[(&[One, Zero], ShotConditions::NONE)]);
    let before = batch.clone();
    for wrong in [&[One][..], &[One, Zero, One]] {
        assert_eq!(
            batch.push_row(wrong, ShotConditions::REJECTED),
            Err(ShapeFault::RowWidth {
                expected: 2,
                got: wrong.len()
            })
        );
        assert_eq!(batch, before);
    }
}

/// Rows come back in the order they were pushed, the batch's shot set's,
/// each with its conditions, clear ones before and after the first that
/// has any, and a lost value in its own slot.
#[test]
fn rows_come_back_in_push_order_with_their_conditions() {
    let batch = readouts(
        2,
        &[
            (&[Zero, Zero], ShotConditions::NONE),
            (&[One, LOST], ShotConditions::UNRESOLVED),
            (&[Zero, One], ShotConditions::NONE),
        ],
    );
    assert_eq!(batch.shots(), 3);
    assert_eq!(batch.row(0), [Zero, Zero]);
    assert_eq!(batch.row(1), [One, LOST]);
    assert_eq!(batch.row(2), [Zero, One]);
    assert_eq!(batch.conditions(0), ShotConditions::NONE);
    assert_eq!(batch.conditions(1), ShotConditions::UNRESOLVED);
    assert_eq!(batch.conditions(2), ShotConditions::NONE);
}

/// Two batches' readouts are equal when their rows and each shot's
/// conditions are, whatever room each was made with, and differ when only
/// a value, or only a shot's conditions, does.
#[test]
fn readouts_are_equal_when_their_rows_and_conditions_are() {
    let rows: [(&[ReadoutValue], ShotConditions); 2] = [
        (&[One], ShotConditions::NONE),
        (&[Zero], ShotConditions::REJECTED),
    ];
    let mut roomy = BatchReadouts::with_capacity(10, 1);
    for (values, conditions) in rows {
        roomy
            .push_row(values, conditions)
            .expect("each row has the batch's width");
    }
    assert_eq!(readouts(1, &rows), roomy);
    let flipped = readouts(
        1,
        &[
            (&[One], ShotConditions::NONE),
            (&[One], ShotConditions::REJECTED),
        ],
    );
    assert_ne!(readouts(1, &rows), flipped);
    let clear = readouts(
        1,
        &[
            (&[One], ShotConditions::NONE),
            (&[Zero], ShotConditions::NONE),
        ],
    );
    assert_ne!(readouts(1, &rows), clear);
}

/// A part reads the readout it asked for by its slot, on a given shot: the
/// value at that slot of that shot's row.
#[test]
fn value_reads_one_slot_of_a_shots_row() {
    let batch = readouts(
        2,
        &[
            (&[One, LOST], ShotConditions::NONE),
            (&[Zero, One], ShotConditions::NONE),
        ],
    );
    assert_eq!(batch.value(0, ReadoutIndex::new(1)), LOST);
    assert_eq!(batch.value(1, ReadoutIndex::new(0)), Zero);
}

/// A batch's readouts, made with its number of shots, take one allocation
/// however many rows are pushed while every shot's conditions are clear:
/// the values' buffer.
#[test]
fn a_full_batch_of_clear_shots_is_one_allocation() {
    let count = allocations(|| {
        let mut batch = BatchReadouts::with_capacity(100, 3);
        for _ in 0..100 {
            batch
                .push_row(&[One, Zero, LOST], ShotConditions::NONE)
                .expect("each row has the batch's width");
        }
        drop(batch);
    });
    assert_eq!(count, 1);
}

/// A batch's readouts, made with its number of shots, take at most two
/// allocations however many rows are pushed, when shots from the middle
/// on have conditions: the values' buffer and the conditions'.
#[test]
fn a_full_batch_with_conditions_is_at_most_two_allocations() {
    let count = allocations(|| {
        let mut batch = BatchReadouts::with_capacity(100, 3);
        for shot in 0..100 {
            let conditions = if shot < 50 {
                ShotConditions::NONE
            } else {
                ShotConditions::REJECTED
            };
            batch
                .push_row(&[One, Zero, LOST], conditions)
                .expect("each row has the batch's width");
        }
        drop(batch);
    });
    assert_eq!(count, 2);
}

/// A caller that clears its readouts between `execute`s, as a driver
/// sending a round at a time does, allocates nothing for an answer the
/// size of one it held before, conditions included.
#[test]
fn cleared_readouts_take_an_answer_of_a_size_they_held_without_allocating() {
    let fill = |readouts: &mut Readouts| {
        for _ in 0..3 {
            let batch = readouts.push_with_capacity(100, 3);
            for shot in 0..100 {
                let conditions = if shot % 2 == 0 {
                    ShotConditions::NONE
                } else {
                    ShotConditions::UNRESOLVED
                };
                batch
                    .push_row(&[One, Zero, LOST], conditions)
                    .expect("each row has the batch's width");
            }
        }
    };
    let mut readouts = Readouts::new();
    fill(&mut readouts);
    let first = readouts.clone();
    readouts.clear();
    let count = allocations(|| fill(&mut readouts));
    assert_eq!(count, 0);
    assert_eq!(readouts, first, "the same answer, in reused memory");
}

/// Cleared readouts that held a small answer take a much larger one as
/// fresh readouts would: at most two allocations, the values' and the
/// conditions', made once with the room for every row.
#[test]
fn cleared_readouts_take_a_larger_answer_in_at_most_two_allocations() {
    let mut readouts = Readouts::new();
    readouts
        .push_with_capacity(1, 3)
        .push_row(&[One, Zero, LOST], ShotConditions::NONE)
        .expect("each row has the batch's width");
    readouts.clear();
    let count = allocations(|| {
        let batch = readouts.push_with_capacity(1000, 3);
        for shot in 0..1000 {
            let conditions = if shot == 0 {
                ShotConditions::REJECTED
            } else {
                ShotConditions::NONE
            };
            batch
                .push_row(&[One, Zero, LOST], conditions)
                .expect("each row has the batch's width");
        }
    });
    assert!(count <= 2, "{count} allocations");
}

/// Two `Readouts` are equal when they hold equal batches' readouts in the
/// same order, whatever memory each keeps, and differ when one batch's do.
#[test]
fn readouts_lists_are_equal_when_their_batches_are() {
    let fill = |readouts: &mut Readouts, values: [&[ReadoutValue]; 2]| {
        for row in values {
            readouts
                .push_with_capacity(1, 1)
                .push_row(row, ShotConditions::NONE)
                .expect("each row has the batch's width");
        }
    };
    let mut fresh = Readouts::new();
    fill(&mut fresh, [&[One], &[Zero]]);
    let mut reused = Readouts::new();
    fill(&mut reused, [&[Zero], &[Zero]]);
    fill(&mut reused, [&[Zero], &[Zero]]);
    reused.clear();
    fill(&mut reused, [&[One], &[Zero]]);
    assert_eq!(fresh, reused);
    let mut other = Readouts::new();
    fill(&mut other, [&[One], &[One]]);
    assert_ne!(fresh, other);
}

/// A batch with no readout slots, such as one that only applies gates,
/// still has a row per shot, carrying its conditions.
#[test]
fn a_batch_of_no_slots_has_a_row_per_shot() {
    let batch = readouts(
        0,
        &[(&[], ShotConditions::NONE), (&[], ShotConditions::REJECTED)],
    );
    assert_eq!(batch.shots(), 2);
    assert!(batch.row(1).is_empty());
    assert_eq!(batch.conditions(1), ShotConditions::REJECTED);
}

/// A slot beyond R would read the next shot's value: it panics.
#[test]
#[should_panic(expected = "slot 2 is beyond the rows' 2 readout slots")]
fn a_slot_beyond_the_rows_panics() {
    let batch = readouts(
        2,
        &[
            (&[One, Zero], ShotConditions::NONE),
            (&[Zero, One], ShotConditions::NONE),
        ],
    );
    let _ = batch.value(0, ReadoutIndex::new(2));
}

/// A shot position beyond the rows panics, even where the rows hold no
/// value to read.
#[test]
#[should_panic(expected = "shot position 1 is beyond the readouts' 1 rows")]
fn a_shot_beyond_the_rows_panics() {
    let batch = readouts(0, &[(&[], ShotConditions::NONE)]);
    let _ = batch.row(1);
}

/// A shot position beyond the rows has no conditions to read.
#[test]
#[should_panic(expected = "shot position 2 is beyond the readouts' 2 rows")]
fn conditions_beyond_the_rows_panic() {
    let batch = readouts(
        1,
        &[
            (&[One], ShotConditions::NONE),
            (&[Zero], ShotConditions::NONE),
        ],
    );
    let _ = batch.conditions(2);
}

/// A stepping debugger sends a batch's instructions in parts on the same
/// shots: joined, each shot's row is its parts' rows, in the parts' order.
#[test]
fn concat_joins_each_shots_rows_in_order() {
    let first = readouts(
        1,
        &[
            (&[One], ShotConditions::NONE),
            (&[Zero], ShotConditions::NONE),
        ],
    );
    let second = readouts(
        2,
        &[
            (&[Zero, LOST], ShotConditions::NONE),
            (&[One, One], ShotConditions::NONE),
        ],
    );
    let joined = BatchReadouts::concat_readouts(&[first, second]);
    let expected = readouts(
        3,
        &[
            (&[One, Zero, LOST], ShotConditions::NONE),
            (&[Zero, One, One], ShotConditions::NONE),
        ],
    );
    assert_eq!(joined, expected);
}

/// A condition any part raised on a shot stays raised on the joined row:
/// each shot's conditions are the union of its parts'.
#[test]
fn concat_unions_each_shots_conditions() {
    let first = readouts(
        0,
        &[(&[], ShotConditions::REJECTED), (&[], ShotConditions::NONE)],
    );
    let second = readouts(
        0,
        &[
            (&[], ShotConditions::UNRESOLVED),
            (&[], ShotConditions::NONE),
        ],
    );
    let joined = BatchReadouts::concat_readouts(&[first, second]);
    assert_eq!(
        joined.conditions(0),
        ShotConditions::REJECTED | ShotConditions::UNRESOLVED
    );
    assert_eq!(joined.conditions(1), ShotConditions::NONE);
}

/// A batch sent below in one part gets its readouts back unchanged.
#[test]
fn concatenating_one_part_gives_it_back() {
    let part = readouts(
        2,
        &[
            (&[One, LOST], ShotConditions::REJECTED),
            (&[Zero, One], ShotConditions::NONE),
        ],
    );
    assert_eq!(
        BatchReadouts::concat_readouts(std::slice::from_ref(&part)),
        part
    );
}

/// Parts of no rows join into no rows, of their summed width.
#[test]
fn concatenating_parts_of_no_rows_gives_no_rows_of_the_summed_width() {
    let joined = BatchReadouts::concat_readouts(&[
        BatchReadouts::with_capacity(0, 2),
        BatchReadouts::with_capacity(0, 3),
    ]);
    assert_eq!(joined.shots(), 0);
    assert_eq!(joined.readouts(), 5);
}

/// Parts that answer different numbers of shots cannot be one batch's,
/// whether the later part answers more shots than the first or fewer.
#[test]
fn concat_panics_on_parts_of_other_shots() {
    let one = || readouts(1, &[(&[One], ShotConditions::NONE)]);
    let two = || {
        readouts(
            1,
            &[
                (&[One], ShotConditions::NONE),
                (&[Zero], ShotConditions::NONE),
            ],
        )
    };
    for (row, parts, expected) in [
        (
            "a later part of more shots",
            [one(), two()],
            "of 1 and of 2",
        ),
        (
            "a later part of fewer shots",
            [two(), one()],
            "of 2 and of 1",
        ),
    ] {
        let panic =
            std::panic::catch_unwind(|| BatchReadouts::concat_readouts(&parts)).expect_err(row);
        let message = panic.downcast_ref::<String>().map(String::as_str);
        assert!(
            message.is_some_and(|m| m.contains(&format!("readouts {expected} shots cannot be joined"))),
            "{row}: {message:?}"
        );
    }
}

/// No parts have no shots to join.
#[test]
#[should_panic(expected = "no readouts to join")]
fn concat_panics_on_no_parts() {
    let _ = BatchReadouts::concat_readouts(&[]);
}

/// The joined width, R, is a `u32`: parts whose slots add up beyond it
/// cannot be joined, rather than wrap to a narrower width. Parts of no rows
/// allocate nothing, whatever their width.
#[test]
#[should_panic(expected = "add up to at most u32::MAX")]
fn concat_panics_when_the_slots_exceed_u32() {
    let wide = BatchReadouts::with_capacity(0, u32::MAX);
    let one = BatchReadouts::with_capacity(0, 1);
    let _ = BatchReadouts::concat_readouts(&[wide, one]);
}

/// Once raised, a condition stays raised: merging another, or none, keeps
/// it.
#[test]
fn merging_conditions_keeps_every_one_raised() {
    let mut conditions = ShotConditions::REJECTED;
    conditions |= ShotConditions::NONE;
    assert_eq!(conditions, ShotConditions::REJECTED);
    conditions |= ShotConditions::INCONSISTENT;
    assert!(conditions.contains(ShotConditions::REJECTED));
    assert!(conditions.contains(ShotConditions::INCONSISTENT));
    assert_eq!(
        conditions | ShotConditions::NONE,
        ShotConditions::REJECTED | ShotConditions::INCONSISTENT
    );
}

/// `contains` holds when every condition asked for is raised, not any of
/// them; asking for none holds for every shot, clear or not.
#[test]
fn contains_holds_when_every_condition_asked_is_raised() {
    let rejected = ShotConditions::REJECTED;
    for (row, raised, asked, holds) in [
        (
            "one raised, one not",
            rejected,
            ShotConditions::REJECTED | ShotConditions::UNRESOLVED,
            false,
        ),
        (
            "one not raised",
            rejected,
            ShotConditions::UNRESOLVED,
            false,
        ),
        ("the one raised", rejected, ShotConditions::REJECTED, true),
        (
            "none, of a raised set",
            rejected,
            ShotConditions::NONE,
            true,
        ),
        (
            "none, of a clear set",
            ShotConditions::NONE,
            ShotConditions::NONE,
            true,
        ),
    ] {
        assert_eq!(raised.contains(asked), holds, "{row}");
    }
}

/// Only a shot with no condition is clear.
#[test]
fn only_no_condition_is_empty() {
    assert!(ShotConditions::NONE.is_empty());
    assert!(ShotConditions::default().is_empty());
    for raised in [
        ShotConditions::REJECTED,
        ShotConditions::INCONSISTENT,
        ShotConditions::UNRESOLVED,
    ] {
        assert!(!raised.is_empty());
    }
}

/// A failing test names the conditions raised.
#[test]
fn debug_names_the_conditions() {
    assert_eq!(
        format!("{:?}", ShotConditions::NONE),
        "ShotConditions(NONE)"
    );
    assert_eq!(
        format!(
            "{:?}",
            ShotConditions::UNRESOLVED | ShotConditions::REJECTED
        ),
        "ShotConditions(REJECTED | UNRESOLVED)"
    );
}

/// A failing test shows the readouts by shot.
#[test]
fn debug_shows_the_rows_by_shot() {
    let batch = readouts(1, &[(&[One], ShotConditions::INCONSISTENT)]);
    assert_eq!(
        format!("{batch:?}"),
        "BatchReadouts { readouts: 1, rows: [([One], ShotConditions(INCONSISTENT))] }"
    );
}

/// A measured bit converts to its value.
#[test]
fn a_bit_converts_to_its_value() {
    assert_eq!(ReadoutValue::from(true), One);
    assert_eq!(ReadoutValue::from(false), Zero);
}
