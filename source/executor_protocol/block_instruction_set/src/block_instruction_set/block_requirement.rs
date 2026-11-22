// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the shots require of a receiver, as an instruction's check reads
//! it ([`BlockRequirement`]).

/// What the shots require of a receiver, as an instruction's check reads
/// it: their blocks of each of the receiver's block types, and whether
/// they may hold readout parities. Prepared once per run from the required
/// [`BlockCapabilities`](crate::BlockCapabilities).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockRequirement {
    /// The shot's blocks of each of the set's block types, in the set's
    /// order.
    pub(crate) blocks_of: Vec<u32>,
    /// Whether an instruction may hold a readout parity
    /// ([`Bit::Parity`](crate::Bit::Parity)).
    pub(crate) readout_parity: bool,
}
