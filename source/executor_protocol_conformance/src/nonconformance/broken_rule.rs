// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`BrokenRule`]: a rule a part broke, as a report lists it.

use crate::nonconformance::Fault;
use crate::rule::Rule;

/// A rule the part broke, and every place where it broke it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokenRule {
    /// The rule.
    pub rule: Rule,
    /// Every place where the part broke it, in the order the check found
    /// them.
    pub faults: Vec<Fault>,
}
