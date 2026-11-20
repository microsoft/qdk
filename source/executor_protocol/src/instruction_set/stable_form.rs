// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A set's stable form, with the `serde` feature: its declaration alone
//! (its name, its block types, each with its qubits, and its
//! instructions, in order), for crossing a process boundary. What a set
//! derives from its declaration is derived again when it is read back,
//! through [`InstructionSet::new`], so a decoded set is checked as a built
//! one is.

use super::{BlockType, InstructionDef, InstructionSet};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

/// A set's declaration, as [`InstructionSet::new`] takes it: borrowed
/// from a set to write it, owned when read.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration<'a> {
    name: Cow<'a, str>,
    block_types: Cow<'a, [(BlockType, u32)]>,
    instructions: Cow<'a, [InstructionDef]>,
}

/// Writes the declaration alone.
impl Serialize for InstructionSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Declaration {
            name: Cow::Borrowed(&self.name),
            block_types: Cow::Borrowed(&self.block_types),
            instructions: Cow::Borrowed(&self.instructions),
        }
        .serialize(serializer)
    }
}

/// Builds the set its declaration writes, with [`InstructionSet::new`];
/// a faulty declaration is refused, naming every fault.
impl<'de> Deserialize<'de> for InstructionSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Declaration {
            name,
            block_types,
            instructions,
        } = Declaration::deserialize(deserializer)?;
        InstructionSet::new(name, block_types.into_owned(), instructions.into_owned()).map_err(
            |faults| {
                let faults: Vec<String> = faults.iter().map(ToString::to_string).collect();
                serde::de::Error::custom(format_args!(
                    "the set's declaration is refused: {}",
                    faults.join("; ")
                ))
            },
        )
    }
}
