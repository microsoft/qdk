// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`BitReadout`]: the readout value of every example set.

/// A measured bit, or no bit: lost on the way, or never written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BitReadout {
    Zero,
    One,
    Lost,
    Unwritten,
}
