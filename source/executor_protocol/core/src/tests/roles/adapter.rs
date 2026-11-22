// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Adapter`'s defaults state: no limit, and no change to what
//! arrives.

use super::every_field_stated;
use crate::tests::fake_parts::{Log, Profiler};
use crate::{Adapter, Capabilities};
use std::cell::Cell;
use std::rc::Rc;

/// An adapter that counts what passes through and changes nothing, such as
/// a profiler, keeps the default `provides`: it limits nothing, so a chain
/// is never refused at its boundary. A default stating any limit, even
/// one as wide as a type's largest value, fails.
#[test]
fn by_default_provides_limits_nothing() {
    let profiler = Profiler::new(&Rc::new(Cell::new(0)), &Log::default());
    assert_eq!(Adapter::provides(&profiler), &Capabilities::UNCONSTRAINED);
}

/// The same profiler keeps the default `requires`: it requires of the part
/// below what arrives from above, every field as it arrived. A default
/// dropping a field, adding one, or changing a value fails.
#[test]
fn by_default_requires_passes_what_arrives_unchanged() {
    let profiler = Profiler::new(&Rc::new(Cell::new(0)), &Log::default());
    let above = every_field_stated();
    assert_eq!(Adapter::requires(&profiler, &above), above);
}
