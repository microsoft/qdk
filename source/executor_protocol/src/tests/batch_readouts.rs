// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A batch's readouts hold a row of exactly R values per shot, in push
//! order, with each shot's conditions; `concat_readouts` joins the parts of
//! one batch; conditions merge by union and are never cleared.

use crate::{
    BatchReadouts, ReadoutFailure, ReadoutIndex, ReadoutValue, ShapeFault, ShotConditions,
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
/// each with its conditions, and a lost value in its own slot.
#[test]
fn rows_are_read_by_position_in_push_order() {
    let batch = readouts(
        2,
        &[
            (&[One, LOST], ShotConditions::UNRESOLVED),
            (&[Zero, One], ShotConditions::NONE),
        ],
    );
    assert_eq!(batch.shots(), 2);
    assert_eq!(batch.readouts(), 2);
    assert_eq!(batch.row(0), [One, LOST]);
    assert_eq!(batch.row(1), [Zero, One]);
    assert_eq!(batch.conditions(0), ShotConditions::UNRESOLVED);
    assert_eq!(batch.conditions(1), ShotConditions::NONE);
    assert_eq!(batch.value(1, ReadoutIndex::new(0)), Zero);
    assert_eq!(batch.value(0, ReadoutIndex::new(1)), LOST);
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
#[should_panic(expected = "slot 2 is beyond the batch's 2 readout slots")]
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
/// shots: joined, each shot's row is its parts' rows in order, and its
/// conditions their union.
#[test]
fn concat_joins_each_shots_rows_and_conditions() {
    let first = readouts(
        1,
        &[
            (&[One], ShotConditions::REJECTED),
            (&[Zero], ShotConditions::NONE),
        ],
    );
    let second = readouts(
        2,
        &[
            (&[Zero, LOST], ShotConditions::UNRESOLVED),
            (&[One, One], ShotConditions::NONE),
        ],
    );
    let joined = BatchReadouts::concat_readouts(&[first, second]);
    let expected = readouts(
        3,
        &[
            (
                &[One, Zero, LOST],
                ShotConditions::REJECTED | ShotConditions::UNRESOLVED,
            ),
            (&[Zero, One, One], ShotConditions::NONE),
        ],
    );
    assert_eq!(joined, expected);
}

/// Parts that answer different numbers of shots cannot be one batch's.
#[test]
#[should_panic(expected = "readouts of 1 and of 2 shots cannot be joined")]
fn concat_panics_on_parts_of_other_shots() {
    let one = readouts(1, &[(&[One], ShotConditions::NONE)]);
    let two = readouts(
        1,
        &[
            (&[One], ShotConditions::NONE),
            (&[Zero], ShotConditions::NONE),
        ],
    );
    let _ = BatchReadouts::concat_readouts(&[one, two]);
}

/// No parts have no shots to join.
#[test]
#[should_panic(expected = "no readouts to join")]
fn concat_panics_on_no_parts() {
    let _ = BatchReadouts::concat_readouts(&[]);
}

/// Once raised, a condition stays raised: merging another, or none, keeps
/// it.
#[test]
fn conditions_merge_by_union_and_are_never_cleared() {
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

/// `contains` asks for every condition given, not any of them, and holds
/// for none.
#[test]
fn contains_needs_every_condition() {
    let rejected = ShotConditions::REJECTED;
    assert!(!rejected.contains(ShotConditions::REJECTED | ShotConditions::UNRESOLVED));
    assert!(!rejected.contains(ShotConditions::UNRESOLVED));
    assert!(rejected.contains(ShotConditions::NONE));
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

/// A value takes a byte or two, so a batch's rows stay dense.
#[test]
fn a_value_is_at_most_two_bytes() {
    assert!(size_of::<ReadoutValue>() <= 2);
}

/// A measured bit converts to its value.
#[test]
fn a_bit_converts_to_its_value() {
    assert_eq!(ReadoutValue::from(true), One);
    assert_eq!(ReadoutValue::from(false), Zero);
}
