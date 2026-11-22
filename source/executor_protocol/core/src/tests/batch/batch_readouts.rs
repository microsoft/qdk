// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A batch's readouts hold a row per shot, by its position, of exactly one
//! value per readout slot, with each shot's conditions, shaped unwritten
//! and clear; `concat_from` joins the parts of one batch, and their
//! conditions.

mod readouts;

use crate::tests::allocations;
use crate::tests::example_sets::Measuring;
use crate::tests::example_sets::{BitReadout, ShotFlags};
use crate::{BatchReadouts, ReadoutSlot};

use BitReadout::{One, Zero};

/// A lost value.
const LOST: BitReadout = BitReadout::Lost;

/// A value no part wrote.
const UNWRITTEN: BitReadout = BitReadout::Unwritten;

/// Readouts of `readout_slots` slots, holding `rows` with their
/// conditions, each written as an executor writes its readouts.
fn readouts(readout_slots: u32, rows: &[(&[BitReadout], ShotFlags)]) -> BatchReadouts<Measuring> {
    let mut batch = BatchReadouts::<Measuring>::new(rows.len(), readout_slots);
    fill(&mut batch, rows);
    batch
}

/// Writes `rows` into `batch`, with their conditions.
fn fill(batch: &mut BatchReadouts<Measuring>, rows: &[(&[BitReadout], ShotFlags)]) {
    for (position, (values, conditions)) in rows.iter().enumerate() {
        batch.row_mut(position).copy_from_slice(values);
        batch.set_conditions(position, *conditions);
    }
}

/// New readouts are shaped as the protocol shapes a batch's: a row per
/// shot, of a value per readout slot, each
/// the set's unwritten value, so that a value no part wrote reads as
/// none, and each shot clear; a batch with no readout slots, such as
/// one that only applies gates, still has a row per shot, each clear.
#[test]
fn new_readouts_are_unwritten_and_clear() {
    for (shots, slots) in [(3, 2), (2, 0)] {
        let batch = BatchReadouts::<Measuring>::new(shots, slots);
        let row = format!("{shots} x {slots}");
        assert_eq!(batch.shots(), shots, "{row}");
        assert_eq!(batch.readout_slots(), slots, "{row}");
        for position in 0..shots {
            assert_eq!(
                batch.row(position),
                vec![UNWRITTEN; slots as usize],
                "{row}, shot {position}"
            );
            assert_eq!(
                batch.conditions(position),
                ShotFlags::NONE,
                "{row}, shot {position}"
            );
        }
    }
}

/// Readouts of more values than a `Vec` holds, `shots` rows of
/// `readout_slots` values counting past a `usize`, panic rather than
/// wrap round to fewer values than their rows read, naming the shots.
#[test]
#[should_panic(expected = "9223372036854775808 shots")]
fn readouts_of_more_values_than_a_vec_holds_panic() {
    let _ = BatchReadouts::<Measuring>::new(usize::MAX / 2 + 1, 2);
}

/// Each row comes back at the position it was written at, the batch's shot
/// set's, with its conditions, clear ones before and after the first that
/// has any, and a lost value in its own slot.
#[test]
fn rows_come_back_by_position_with_their_conditions() {
    let batch = readouts(
        2,
        &[
            (&[Zero, Zero], ShotFlags::NONE),
            (&[One, LOST], ShotFlags::UNRESOLVED),
            (&[Zero, One], ShotFlags::NONE),
        ],
    );
    assert_eq!(batch.shots(), 3);
    assert_eq!(batch.row(0), [Zero, Zero]);
    assert_eq!(batch.row(1), [One, LOST]);
    assert_eq!(batch.row(2), [Zero, One]);
    assert_eq!(batch.conditions(0), ShotFlags::NONE);
    assert_eq!(batch.conditions(1), ShotFlags::UNRESOLVED);
    assert_eq!(batch.conditions(2), ShotFlags::NONE);
}

/// Two batches' readouts are equal when their rows and each shot's
/// conditions are, whatever memory each keeps (a shot whose condition was
/// raised, then cleared, as an adapter that resolves it clears it, equals
/// one never raised), and differ when only a value, a shot's conditions,
/// the number of rows or the readout slots do: also where no value
/// differs, as for rows of no values, of a batch that only applies gates,
/// or for readouts of no rows.
#[test]
fn readouts_are_equal_when_their_rows_and_conditions_are() {
    let rows: [(&[BitReadout], ShotFlags); 2] =
        [(&[One], ShotFlags::NONE), (&[Zero], ShotFlags::REJECTED)];
    let clear_rows: [(&[BitReadout], ShotFlags); 2] =
        [(&[One], ShotFlags::NONE), (&[Zero], ShotFlags::NONE)];
    let mut cleared = readouts(1, &clear_rows);
    cleared.set_conditions(0, ShotFlags::INCONSISTENT);
    cleared.set_conditions(0, ShotFlags::NONE);
    let flipped = readouts(
        1,
        &[(&[One], ShotFlags::NONE), (&[One], ShotFlags::REJECTED)],
    );
    for (row, left, right, equal) in [
        (
            "a condition raised, then cleared",
            readouts(1, &clear_rows),
            cleared,
            true,
        ),
        ("a value differs", readouts(1, &rows), flipped, false),
        (
            "a shot's conditions differ",
            readouts(1, &rows),
            readouts(1, &clear_rows),
            false,
        ),
        (
            "2 or 3 rows of no values",
            BatchReadouts::<Measuring>::new(2, 0),
            BatchReadouts::<Measuring>::new(3, 0),
            false,
        ),
        (
            "no rows, of 1 or 2 values",
            BatchReadouts::<Measuring>::new(0, 1),
            BatchReadouts::<Measuring>::new(0, 2),
            false,
        ),
    ] {
        assert_eq!(left == right, equal, "{row}: {left:?} and {right:?}");
    }
}

/// A part reads the readout it asked for by its slot, on a given shot: the
/// value at that slot of that shot's row.
#[test]
fn value_reads_one_slot_of_a_shots_row() {
    let batch = readouts(
        2,
        &[
            (&[One, LOST], ShotFlags::NONE),
            (&[Zero, One], ShotFlags::NONE),
        ],
    );
    assert_eq!(batch.value(0, ReadoutSlot::new(1)), LOST);
    assert_eq!(batch.value(1, ReadoutSlot::new(0)), Zero);
}

/// A batch's readouts take at most one allocation, the values' buffer,
/// however many rows are written while every shot's conditions are clear,
/// each set clear as an adapter forwards them, and at most two, the
/// values' and the conditions', when shots from the middle on have
/// conditions.
#[test]
fn a_batchs_readouts_take_at_most_one_allocation_and_one_more_for_conditions() {
    for (conditioned, most) in [(false, 1), (true, 2)] {
        let count = allocations(|| {
            let mut batch = BatchReadouts::<Measuring>::new(100, 3);
            for position in 0..100 {
                batch.row_mut(position).copy_from_slice(&[One, Zero, LOST]);
                let conditions = if conditioned && position >= 50 {
                    ShotFlags::REJECTED
                } else {
                    ShotFlags::NONE
                };
                batch.set_conditions(position, conditions);
            }
            drop(batch);
        });
        assert!(
            count <= most,
            "conditions: {conditioned}, {count} allocations"
        );
    }
}

/// A slot beyond the rows' readout slots would read the next shot's value:
/// it panics, naming the slot.
#[test]
#[should_panic(expected = "slot 2")]
fn a_slot_beyond_the_rows_panics() {
    let batch = readouts(
        2,
        &[
            (&[One, Zero], ShotFlags::NONE),
            (&[Zero, One], ShotFlags::NONE),
        ],
    );
    let _ = batch.value(0, ReadoutSlot::new(2));
}

/// A shot position beyond the rows has no row to read or write, and no
/// conditions to read or set, even where the rows hold no value, one row
/// or two: each panics, naming the position and the rows.
#[test]
fn a_shot_beyond_the_rows_panics() {
    type Call = Box<dyn FnOnce(&mut BatchReadouts<Measuring>)>;
    let one_row = || readouts(0, &[(&[], ShotFlags::NONE)]);
    let two_rows = || BatchReadouts::<Measuring>::new(2, 0);
    let panics: [(&str, BatchReadouts<Measuring>, Call, [&str; 2]); 5] = [
        (
            "row, of one row",
            one_row(),
            Box::new(|batch| _ = batch.row(1)),
            ["position 1", "1 rows"],
        ),
        (
            "row",
            two_rows(),
            Box::new(|batch| _ = batch.row(2)),
            ["position 2", "2 rows"],
        ),
        (
            "conditions",
            two_rows(),
            Box::new(|batch| _ = batch.conditions(2)),
            ["position 2", "2 rows"],
        ),
        (
            "set_conditions",
            two_rows(),
            Box::new(|batch| batch.set_conditions(2, ShotFlags::NONE)),
            ["position 2", "2 rows"],
        ),
        (
            "row_mut",
            two_rows(),
            Box::new(|batch| _ = batch.row_mut(2)),
            ["position 2", "2 rows"],
        ),
    ];
    for (method, mut batch, call, expected) in panics {
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(&mut batch)))
            .expect_err(method);
        let message = panic.downcast_ref::<String>().map(String::as_str);
        for said in expected {
            assert!(
                message.is_some_and(|m| m.contains(said)),
                "{method}: {message:?} says {said:?}"
            );
        }
    }
}

/// A stepping debugger sends a batch's instructions in parts on the same
/// shots: joined, each shot's row is its parts' rows, in the parts' order;
/// a batch sent below in one part gets that part's readouts back
/// unchanged.
#[test]
fn concat_joins_each_shots_rows_in_order() {
    let first = readouts(1, &[(&[One], ShotFlags::NONE), (&[Zero], ShotFlags::NONE)]);
    let second = readouts(
        2,
        &[
            (&[Zero, LOST], ShotFlags::NONE),
            (&[One, One], ShotFlags::NONE),
        ],
    );
    let alone = readouts(
        2,
        &[
            (&[One, LOST], ShotFlags::REJECTED),
            (&[Zero, One], ShotFlags::NONE),
        ],
    );
    let two_parts = readouts(
        3,
        &[
            (&[One, Zero, LOST], ShotFlags::NONE),
            (&[Zero, One, One], ShotFlags::NONE),
        ],
    );
    for (row, parts, expected) in [
        ("two parts", vec![first, second], two_parts),
        ("one part", vec![alone.clone()], alone),
    ] {
        let mut joined = BatchReadouts::<Measuring>::new(2, expected.readout_slots());
        joined.concat_from(&parts);
        assert_eq!(joined, expected, "{row}");
    }
}

/// A condition any part raised on a shot stays raised on the joined row,
/// and none other: each shot's conditions are its parts' joined (`|=`),
/// whatever the joined readouts held. So a batch of no readout slots sent
/// below in no part, as a splitter of its instructions sends one of no
/// instructions, has its rows' conditions cleared: no part raised any.
#[test]
fn concat_unions_each_shots_conditions() {
    let first = readouts(0, &[(&[], ShotFlags::REJECTED), (&[], ShotFlags::NONE)]);
    let second = readouts(0, &[(&[], ShotFlags::UNRESOLVED), (&[], ShotFlags::NONE)]);
    let mut held_inconsistent = BatchReadouts::<Measuring>::new(2, 0);
    held_inconsistent.set_conditions(1, ShotFlags::INCONSISTENT);
    let held_rejected = readouts(0, &[(&[], ShotFlags::REJECTED), (&[], ShotFlags::NONE)]);
    let union = readouts(
        0,
        &[
            (&[], ShotFlags::REJECTED | ShotFlags::UNRESOLVED),
            (&[], ShotFlags::NONE),
        ],
    );
    for (row, mut joined, parts, expected) in [
        ("two parts", held_inconsistent, vec![first, second], union),
        (
            "no parts",
            held_rejected,
            vec![],
            BatchReadouts::<Measuring>::new(2, 0),
        ),
    ] {
        joined.concat_from(&parts);
        assert_eq!(joined, expected, "{row}");
    }
}

/// Parts that answer different numbers of shots cannot be one batch's,
/// whether the later part answers more shots than the first or fewer:
/// the panic names both counts.
#[test]
fn concat_panics_on_parts_of_other_shots() {
    let one = || readouts(1, &[(&[One], ShotFlags::NONE)]);
    let two = || readouts(1, &[(&[One], ShotFlags::NONE), (&[Zero], ShotFlags::NONE)]);
    for (row, parts, expected) in [
        (
            "a later part of more shots",
            [one(), two()],
            ["2 shots", "of 1"],
        ),
        (
            "a later part of fewer shots",
            [two(), one()],
            ["1 shot", "of 2"],
        ),
    ] {
        let shots = parts[0].shots();
        let panic = std::panic::catch_unwind(|| {
            BatchReadouts::<Measuring>::new(shots, 2).concat_from(&parts);
        })
        .expect_err(row);
        let message = panic.downcast_ref::<String>().map(String::as_str);
        for said in expected {
            assert!(
                message.is_some_and(|m| m.contains(said)),
                "{row}: {message:?} says {said:?}"
            );
        }
    }
}

/// Parts must answer all of the batch's instructions, and no more: parts
/// whose readout slots add up to fewer or more than the rows' cannot fill
/// them, also where they add up beyond a `u32`, of no rows so that
/// `u32::MAX` slots cost no memory: the panic names both counts.
#[test]
fn concat_panics_on_parts_of_other_summed_readout_slots() {
    let part = |shots, slots| BatchReadouts::<Measuring>::new(shots, slots);
    for (row, rows, parts, expected) in [
        (
            "fewer slots",
            part(1, 3),
            vec![part(1, 1), part(1, 1)],
            ["2 readout slots", "rows of 3"],
        ),
        (
            "more slots",
            part(1, 1),
            vec![part(1, 1), part(1, 1)],
            ["2 readout slots", "rows of 1"],
        ),
        (
            "beyond a u32",
            part(0, 0),
            vec![part(0, u32::MAX), part(0, 1)],
            ["4294967296 readout slots", "rows of 0"],
        ),
    ] {
        let mut rows = rows;
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rows.concat_from(&parts);
        }))
        .expect_err(row);
        let message = panic.downcast_ref::<String>().map(String::as_str);
        for said in expected {
            assert!(
                message.is_some_and(|m| m.contains(said)),
                "{row}: {message:?} says {said:?}"
            );
        }
    }
}

/// A failing test shows the readouts by shot: each shot's readouts and
/// conditions, in the shots' order.
#[test]
fn debug_shows_the_rows_by_shot() {
    let shots: [(&[BitReadout], ShotFlags); 2] = [
        (&[One, Zero], ShotFlags::INCONSISTENT),
        (&[Zero, One], ShotFlags::REJECTED),
    ];
    let shown = format!("{:?}", readouts(2, &shots));
    let mut from = 0;
    for (row, conditions) in shots {
        for said in [format!("{row:?}"), format!("{conditions:?}")] {
            let at = shown[from..].find(&said);
            let at = at.unwrap_or_else(|| panic!("{shown} shows {said} after {from}"));
            from += at + said.len();
        }
    }
}
