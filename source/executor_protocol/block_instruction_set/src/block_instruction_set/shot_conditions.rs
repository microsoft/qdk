// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot's conditions, [`ShotConditions`]: what a part found suspect in a
//! shot without ending it.
//!
//! A level of a code rejects a shot whose checks failed, a decoder finds a
//! shot's checks inconsistent, an executor cannot resolve a value its
//! instructions read that counts a lost readout: in each case the shot
//! runs on, and its readouts stand, but whoever reads its results must
//! know they are suspect. So each row of a batch's readouts carries its
//! shot's conditions; a part above
//! must merge those it reads below with its own, by union, and pass them up,
//! so that the driver sees every condition raised on the shot, at any
//! level. A row reports the conditions raised on its shot in that
//! `execute` only, since each `execute` gets its rows clear
//! ([`Executor::execute`](executor_protocol_core::Executor::execute)): a driver that runs a
//! shot over several calls merges each call's conditions into the shot's,
//! by union, as a part above merges its own with those below. This type has
//! no operation that clears a condition, so a union keeps every one raised.

use std::fmt;
use std::ops::{BitOr, BitOrAssign};

/// The conditions raised on a shot: a set, merged by union ([`BitOr`]),
/// with no operation that clears one.
///
/// One byte, behind a private field, so that a condition can be added
/// without breaking any part. Its `Debug` names the conditions, as
/// `ShotConditions(REJECTED | UNRESOLVED)`; its `Default` is
/// [`NONE`](Self::NONE).
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ShotConditions(u8);

impl ShotConditions {
    /// No condition: a clear shot.
    pub const NONE: Self = Self(0);
    /// A check the shot was asked to pass failed, such as a flag a level of
    /// a code post-selects on. The driver chooses what to do with it: one
    /// that post-selects drops the shot.
    pub const REJECTED: Self = Self(1);
    /// The shot has checks that cannot all hold, so its corrected results
    /// may be wrong.
    pub const INCONSISTENT: Self = Self(1 << 1);
    /// A value the shot depends on could not be determined, such as a
    /// value its instructions read, steering a later instruction, that
    /// counts a lost readout.
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

    /// Whether every condition of `other` is raised here: always, for
    /// [`NONE`](Self::NONE).
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
    /// Raises the conditions of `other` here too.
    fn bitor_assign(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl fmt::Debug for ShotConditions {
    /// `ShotConditions(NONE)`, or the conditions raised, joined by ` | `.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ShotConditions(")?;
        if self.is_empty() {
            f.write_str("NONE")?;
        }
        let mut separator = "";
        for (condition, name) in Self::NAMED {
            if self.contains(condition) {
                write!(f, "{separator}{name}")?;
                separator = " | ";
            }
        }
        f.write_str(")")
    }
}
