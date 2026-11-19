// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Raise`] and [`Raised`]: the conditions a check has the kit's executor
//! raise on a shot's rows, and the patterns they follow: one kind after
//! another ([`raised_in_turn`]), kinds drawn ([`drawn_kinds`]), and the
//! halves of the shots raised on ([`raising_halves`]).

use executor_protocol::{ShotConditions, ShotId};

use crate::splitmix64::mix;

/// Conditions the kit's executor raises on a shot's rows, in the calls
/// it gets in an `execute` of the part above it (for a driver's check, in
/// the driver's `execute`), as [`Raised`] says.
#[derive(Clone, Copy)]
pub(crate) struct Raise {
    pub(crate) execute: usize,
    pub(crate) shot: ShotId,
    pub(crate) raised: Raised,
}

/// What a [`Raise`] raises in each call.
#[derive(Clone, Copy)]
pub(crate) enum Raised {
    /// These conditions, in every call.
    InEveryCall(ShotConditions),
    /// These conditions in the first call, and none in the later ones.
    InFirstCall(ShotConditions),
    /// One kind in each call, in turn by the call ([`raised_in_turn`]),
    /// for the shot at `position` among the run's.
    InTurn { position: usize },
    /// One kind or more in each call, drawn from `key`, the `execute`,
    /// the call and the shot ([`drawn_kinds`]).
    Drawn { key: u64 },
}

/// The conditions a check raises in turn, one kind after another, so that
/// a part keeping only some of them shows.
pub(crate) const RAISED_IN_TURN: [ShotConditions; 3] = [
    ShotConditions::REJECTED,
    ShotConditions::INCONSISTENT,
    ShotConditions::UNRESOLVED,
];

/// Every kind in [`RAISED_IN_TURN`], together.
pub(crate) fn every_kind() -> ShotConditions {
    RAISED_IN_TURN
        .into_iter()
        .fold(ShotConditions::NONE, |all, kind| all | kind)
}

/// The kind of condition a check raises at `turn` on the shot at
/// `position` among the run's: [`RAISED_IN_TURN`] in order, starting one
/// kind later for each next position, so that neighbouring positions read
/// different kinds (the kinds repeat every three positions).
pub(crate) fn raised_in_turn(turn: usize, position: usize) -> ShotConditions {
    RAISED_IN_TURN[(turn + position) % RAISED_IN_TURN.len()]
}

/// The kinds a [`Raised::Drawn`] with `key` raises on `shot` in call
/// `call` of `execute`: one of the seven sets of one kind or more of
/// [`RAISED_IN_TURN`], drawn for each `(key, execute, call, shot)`, each
/// set as likely.
pub(crate) fn drawn_kinds(key: u64, execute: usize, call: usize, shot: ShotId) -> ShotConditions {
    let place = (execute as u64) << 40 ^ (call as u64) << 32 ^ u64::from(shot.get());
    let set = 1 + mix(key ^ mix(place)) % 7;
    RAISED_IN_TURN
        .into_iter()
        .enumerate()
        .filter(|(bit, _)| set >> bit & 1 == 1)
        .fold(ShotConditions::NONE, |all, (_, kind)| all | kind)
}

/// The positions, among `count` shots, that a check raises conditions on
/// in each of its raising runs: for each bit of a position, those whose
/// bit is set, then the others, leaving out an empty half. So any two
/// positions are apart in some run, one raised on and the other not, and
/// a part that adds a shot's conditions to another's row shows, in that
/// run, on the row not raised on. One run for one shot, two for two, four
/// for three or four, six for up to eight.
pub(crate) fn raising_halves(count: usize) -> Vec<Vec<usize>> {
    let bits = count.saturating_sub(1).bit_width().max(1);
    (0..bits)
        .flat_map(|bit| [true, false].map(|set| (bit, set)))
        .map(|(bit, set)| {
            (0..count)
                .filter(|position| (position >> bit & 1 == 1) == set)
                .collect::<Vec<_>>()
        })
        .filter(|half| !half.is_empty())
        .collect()
}
