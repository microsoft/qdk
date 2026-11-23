// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What `Adapter`'s defaults state: no limit, and no change to what
//! arrives.

use super::every_field_stated;
use crate::tests::example_sets::QubitLimited;
use crate::tests::fake_parts::{Log, QubitPassThrough};
use crate::{InstructionSet, TranslatingAdapter};

/// An adapter that passes what arrives and changes nothing, such as a
/// profiler, either keeping the default `provides` of its limits or
/// stating none at all, limits nothing, so a chain is never refused at
/// its boundary. A default stating any limit, even one as wide as a
/// type's largest value, fails.
#[test]
fn by_default_provides_limits_nothing() {
    let adapter = QubitPassThrough::new(&Log::default());
    assert_eq!(
        TranslatingAdapter::provides(&adapter),
        QubitLimited::unconstrained()
    );
    assert_eq!(
        TranslatingAdapter::provides(&adapter.part),
        QubitLimited::unconstrained()
    );
}

/// The same adapter, keeping the default `requires` of its limits or
/// stating none at all, requires of the part below what arrives from
/// above, every field as it arrived, the set's qubits included. A default
/// dropping a field, adding one, or changing a value fails.
#[test]
fn by_default_requires_passes_what_arrives_unchanged() {
    let adapter = QubitPassThrough::new(&Log::default());
    let above = every_field_stated();
    assert_eq!(TranslatingAdapter::requires(&adapter, &above), above);
    assert_eq!(TranslatingAdapter::requires(&adapter.part, &above), above);
}
