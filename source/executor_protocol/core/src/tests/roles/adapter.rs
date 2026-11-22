// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Adapter`'s defaults state: no limit, and no change to what
//! arrives.

use super::every_field_stated;
use crate::tests::example_sets::QubitLimited;
use crate::tests::fake_parts::{Log, QubitPassThrough};
use crate::{Adapter, InstructionSet};

/// An adapter that passes what arrives and changes nothing, such as a
/// profiler, keeps the default `provides`: it limits nothing, so a chain
/// is never refused at its boundary. A default stating any limit, even
/// one as wide as a type's largest value, fails.
#[test]
fn by_default_provides_limits_nothing() {
    let adapter = QubitPassThrough::new(&Log::default());
    assert_eq!(Adapter::provides(&adapter), QubitLimited::unconstrained());
}

/// The same adapter keeps the default `requires`: it requires of the part
/// below what arrives from above, every field as it arrived, the set's
/// qubits included. A default dropping a field, adding one, or changing a
/// value fails.
#[test]
fn by_default_requires_passes_what_arrives_unchanged() {
    let adapter = QubitPassThrough::new(&Log::default());
    let above = every_field_stated();
    assert_eq!(Adapter::requires(&adapter, &above), above);
}
