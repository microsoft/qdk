// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`BrokenRule`]: a rule a part broke, as a report lists it.

use crate::nonconformance::Fault;
use crate::rule::Rule;

/// A rule the part broke, and the places where the check found it broken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokenRule {
    /// The rule.
    pub rule: Rule,
    /// The places where the check found the part broke it, in the order it
    /// found them: every place, or as each check's Errors section says.
    pub faults: Vec<Fault>,
}
