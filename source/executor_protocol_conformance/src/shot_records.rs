// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Each shot's records over a run ([`ShotRecords`]): the rows the part
//! under test answered for the shot, in order, each its values and its
//! conditions; and how the checks compare them across runs, naming each
//! row that differs.

use std::collections::BTreeMap;
use std::fmt::Write;

use executor_protocol::{ReadoutFailure, ReadoutValue, ShotConditions, ShotId};

use crate::nonconformance::{Fault, Place};
use crate::shared_run::{RowRecord, RunLog};

/// Each shot's rows over a run, in the order the part answered them.
pub(crate) struct ShotRecords(BTreeMap<ShotId, Vec<RowRecord>>);

impl ShotRecords {
    /// The records of a run's log.
    pub(crate) fn of(log: &RunLog) -> Self {
        let mut records: BTreeMap<ShotId, Vec<RowRecord>> = BTreeMap::new();
        for row in &log.rows {
            records.entry(row.shot).or_default().push(row.clone());
        }
        Self(records)
    }

    /// `shot`'s rows.
    pub(crate) fn rows(&self, shot: ShotId) -> &[RowRecord] {
        self.0.get(&shot).map_or(&[], Vec::as_slice)
    }

    /// `shot`'s values, its rows' joined in order.
    pub(crate) fn values(&self, shot: ShotId) -> Vec<ReadoutValue> {
        self.rows(shot)
            .iter()
            .flat_map(|row| row.values.iter().copied())
            .collect()
    }

    /// `shot`'s conditions, its rows' union.
    pub(crate) fn conditions(&self, shot: ShotId) -> ShotConditions {
        self.rows(shot)
            .iter()
            .fold(ShotConditions::NONE, |all, row| all | row.conditions)
    }

    /// Whether some row holds a value.
    pub(crate) fn reads_anything(&self) -> bool {
        self.0.values().flatten().any(|row| !row.values.is_empty())
    }

    /// Whether `shot` reads otherwise than `other`, row for row.
    pub(crate) fn differ(&self, shot: ShotId, other: ShotId) -> bool {
        let rows = |shot| {
            self.rows(shot)
                .iter()
                .map(|row| (&row.values, row.conditions))
        };
        !rows(shot).eq(rows(other))
    }
}

/// The faults where `shot`'s rows in `found` differ from its rows in
/// `expected`, each named by the label that says which run it is: one at
/// each row that differs, or one at the shot when it has another number
/// of rows.
pub(crate) fn row_faults(
    shot: ShotId,
    (found, found_in): (&ShotRecords, &str),
    (expected, expected_in): (&ShotRecords, &str),
) -> Vec<Fault> {
    let (found, expected) = (found.rows(shot), expected.rows(shot));
    if found.len() != expected.len() {
        return vec![Fault {
            place: Place::Shot { shot },
            found: format!(
                "{} rows {found_in}, {} rows {expected_in}",
                found.len(),
                expected.len()
            ),
        }];
    }
    found
        .iter()
        .zip(expected)
        .filter(|(found, expected)| {
            found.values != expected.values || found.conditions != expected.conditions
        })
        .map(|(found, expected)| Fault {
            place: Place::Row {
                execute: found.execute,
                batch: found.batch,
                row: found.row,
                shot,
            },
            found: format!(
                "reads {} {found_in}, {} {expected_in}",
                show(&found.values, found.conditions),
                show(&expected.values, expected.conditions)
            ),
        })
        .collect()
}

/// The fault where `shot`'s values and conditions over `found`'s run,
/// joined, differ from `expected`'s, if they do.
pub(crate) fn joined_fault(
    shot: ShotId,
    (found, found_in): (&ShotRecords, &str),
    (expected, expected_in): (&ShotRecords, &str),
) -> Option<Fault> {
    let joined = |records: &ShotRecords| (records.values(shot), records.conditions(shot));
    let (found, expected) = (joined(found), joined(expected));
    (found != expected).then(|| Fault {
        place: Place::Shot { shot },
        found: format!(
            "reads {} {found_in}, {} {expected_in}",
            show(&found.0, found.1),
            show(&expected.0, expected.1)
        ),
    })
}

/// Values and conditions, short: `[01?]`, then the conditions if any are
/// raised. `?` is a value missing for another reason than a loss, `L` one
/// lost.
pub(crate) fn show(values: &[ReadoutValue], conditions: ShotConditions) -> String {
    let mut shown = String::from("[");
    for value in values {
        shown.push(match value {
            ReadoutValue::Zero => '0',
            ReadoutValue::One => '1',
            ReadoutValue::Missing(ReadoutFailure::Loss) => 'L',
            _ => '?',
        });
    }
    shown.push(']');
    if !conditions.is_empty() {
        let _ = write!(shown, " {conditions:?}");
    }
    shown
}
