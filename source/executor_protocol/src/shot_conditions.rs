// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's conditions: what a component found wrong with a shot without
//! ending it, such as a code's runtime that rejects a shot, finds its
//! checks inconsistent, or cannot determine a value. They are per shot and
//! merge by union: once raised, a condition stays raised for the shot. Each
//! outcome carries its shot's conditions so far
//! ([`ShotOutcome::conditions`]), as a code's runtime marks each result
//! with its status (`docs/replies.md`, §Shot conditions).

use std::fmt;
use std::ops::{BitOr, BitOrAssign};

#[cfg(doc)]
use crate::ShotOutcome;

/// The conditions raised on a shot, a set: each says that the shot's
/// results are suspect in its own way, while the shot runs on and its
/// readouts stand. A component raises them on the outcomes it gives; an
/// [`adapter_fn`](fn@crate::adapter_fn) above merges those it reads below
/// with its own, by union ([`BitOr`]), and marks its outcomes with them, so
/// that the caller sees each shot's conditions on its outcomes whatever
/// layer raised them.
///
/// A set of flags in one byte, so that an outcome stays as small as it was
/// without them, with room for eight; its field is private, so that adding
/// a condition breaks no caller. Its `Debug` names the flags, as in
/// `ShotConditions(REJECTED | UNRESOLVED)`.
///
/// ```
/// use executor_protocol::ShotConditions;
///
/// let conditions = ShotConditions::REJECTED | ShotConditions::UNRESOLVED;
/// assert!(conditions.contains(ShotConditions::REJECTED));
/// assert!(!conditions.contains(ShotConditions::INCONSISTENT));
/// assert!(conditions.contains(ShotConditions::NONE));
/// ```
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ShotConditions(u8);

impl ShotConditions {
    /// No condition: a clear shot.
    pub const NONE: Self = Self(0);
    /// The shot was flagged for rejection: a check it was asked to pass
    /// failed (a post-selection, a flag). The caller chooses what to do
    /// with it: a caller that post-selects drops the shot.
    pub const REJECTED: Self = Self(1);
    /// The shot's checks disagree with each other (a parity that cannot
    /// hold), so its corrected results may be wrong.
    pub const INCONSISTENT: Self = Self(1 << 1);
    /// A value the shot depends on could not be determined (a readout lost
    /// or never read, a check that could not be decided).
    pub const UNRESOLVED: Self = Self(1 << 2);

    /// The named conditions, in the order `Debug` lists them.
    const NAMED: [(Self, &'static str); 3] = [
        (Self::REJECTED, "REJECTED"),
        (Self::INCONSISTENT, "INCONSISTENT"),
        (Self::UNRESOLVED, "UNRESOLVED"),
    ];

    /// Whether no condition is raised.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every condition of `other` is raised in this set: always,
    /// for [`NONE`](Self::NONE).
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for ShotConditions {
    type Output = Self;

    /// The union: the conditions raised in either.
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitOrAssign for ShotConditions {
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl fmt::Debug for ShotConditions {
    /// `ShotConditions(NONE)`, or its flags joined by ` | `.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ShotConditions(")?;
        if self.is_empty() {
            f.write_str("NONE")?;
        }
        let mut sep = "";
        for (flag, name) in Self::NAMED {
            if self.contains(flag) {
                write!(f, "{sep}{name}")?;
                sep = " | ";
            }
        }
        f.write_str(")")
    }
}
