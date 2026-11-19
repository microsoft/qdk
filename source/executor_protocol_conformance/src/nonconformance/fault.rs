// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Fault`]: one place where a part broke a rule, and what the check found
//! there.

use std::fmt;

use crate::nonconformance::Place;

/// One place where a part broke a rule, and what the check found there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    /// Where.
    pub place: Place,
    /// What the check found there, against what the rule asks.
    pub found: String,
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.place, self.found)
    }
}

/// `shots`, for a fault's text: `shot 0` for one, `shots 0, 2` for more.
pub(crate) fn shot_list(shots: &[String]) -> String {
    match shots {
        [shot] => format!("shot {shot}"),
        shots => format!("shots {}", shots.join(", ")),
    }
}
