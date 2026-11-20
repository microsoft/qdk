// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A set's stable form, with the `serde` feature: the set's declaration
//! alone, its name, its block types and its instructions, in order, each
//! an [`InstructionDef`]. Its shape is in `docs/instruction-sets.md`;
//! what the form promises is [`InstructionSet`]'s to say.

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

/// Builds the set its declaration writes, with [`InstructionSet::new`].
impl<'de> Deserialize<'de> for InstructionSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Declaration {
            name,
            block_types,
            instructions,
        } = Declaration::deserialize(deserializer)?;
        let name = name.into_owned();
        InstructionSet::new(
            name.clone(),
            block_types.into_owned(),
            instructions.into_owned(),
        )
        .map_err(|faults| {
            let faults: Vec<String> = faults.iter().map(ToString::to_string).collect();
            serde::de::Error::custom(format_args!(
                "the declaration of set `{name}` is refused: {}",
                faults.join("; ")
            ))
        })
    }
}
