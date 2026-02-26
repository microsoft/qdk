// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    /// QIR attributes used during codegen.
    pub struct Attributes: u32 {
        const EntryPoint   = 0b_0001;
        const Irreversible = 0b_0010;
        const QdkNoise     = 0b_0100;
    }
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes::EntryPoint | Attributes::Irreversible
    }
}
