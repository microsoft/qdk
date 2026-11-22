// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Wide`]: each instruction the count of readout slots it writes.

use std::convert::Infallible;
use std::error::Error;
use std::fmt;

use super::{BitReadout, NoCapabilities, ShotFlags};
use crate::{BoundaryCompatible, Capabilities, Features, InstructionSet, ReadoutSlots, Resources};

/// A set whose each instruction is the count of readout slots it writes,
/// up to `u32::MAX - 1`; `u32::MAX` is refused ([`Beyond`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Wide;

impl BoundaryCompatible for Wide {
    type Fit = ();
    type Mismatch = Infallible;

    fn fits(&self, _accepted: &Self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl InstructionSet for Wide {
    type Instruction = u32;
    type Capabilities = NoCapabilities;
    type Requirement = ();
    type Fault = Beyond;
    type Readout = BitReadout;
    type ShotConditions = ShotFlags;
    const UNWRITTEN_READOUT: BitReadout = BitReadout::Unwritten;

    fn unconstrained() -> &'static Capabilities<Self> {
        static UNCONSTRAINED: Capabilities<Wide> = Capabilities::new(
            Resources::unconstrained(),
            Features::unconstrained(),
            NoCapabilities,
        );
        &UNCONSTRAINED
    }

    fn readout_slots(&self, slots: &u32) -> ReadoutSlots {
        *slots
    }

    fn requirement(&self, _required: &NoCapabilities) {}

    fn check(&self, slots: &u32, (): &()) -> Result<ReadoutSlots, Beyond> {
        if *slots == u32::MAX {
            Err(Beyond)
        } else {
            Ok(*slots)
        }
    }
}

/// The instruction `u32::MAX`, which [`Wide`] refuses.
#[derive(Debug)]
pub(crate) struct Beyond;

impl fmt::Display for Beyond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("beyond the set")
    }
}

impl Error for Beyond {}
