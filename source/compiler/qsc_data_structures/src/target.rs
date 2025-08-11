// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct TargetCapabilityFlags: u32 {
        const Adaptive = 0b0000_0001;
        const IntegerComputations = 0b0000_0010;
        const FloatingPointComputations = 0b0000_0100;
        const BackwardsBranching = 0b0000_1000;
        const HigherLevelConstructs = 0b0001_0000;
        const QubitReset = 0b0010_0000;
    }
}

impl std::str::FromStr for TargetCapabilityFlags {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "Base" => Ok(TargetCapabilityFlags::empty()),
            "Adaptive" => Ok(TargetCapabilityFlags::Adaptive),
            "IntegerComputations" => Ok(TargetCapabilityFlags::IntegerComputations),
            "FloatingPointComputations" => Ok(TargetCapabilityFlags::FloatingPointComputations),
            "BackwardsBranching" => Ok(TargetCapabilityFlags::BackwardsBranching),
            "HigherLevelConstructs" => Ok(TargetCapabilityFlags::HigherLevelConstructs),
            "QubitReset" => Ok(TargetCapabilityFlags::QubitReset),
            "Unrestricted" => Ok(TargetCapabilityFlags::all()),
            _ => Err(()),
        }
    }
}

impl Default for TargetCapabilityFlags {
    fn default() -> Self {
        TargetCapabilityFlags::empty()
    }
}

use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    Unrestricted,
    Base,
    AdaptiveRI,
    AdaptiveRIF,
}

impl Profile {
    #[must_use]
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Unrestricted => "Unrestricted",
            Self::Base => "Base",
            Self::AdaptiveRI => "Adaptive_RI",
            Self::AdaptiveRIF => "Adaptive_RIF",
        }
    }
}

impl From<Profile> for TargetCapabilityFlags {
    fn from(value: Profile) -> Self {
        match value {
            Profile::Unrestricted => Self::all(),
            Profile::Base => Self::empty(),
            Profile::AdaptiveRI => Self::Adaptive | Self::QubitReset | Self::IntegerComputations,
            Profile::AdaptiveRIF => {
                Self::Adaptive
                    | Self::QubitReset
                    | Self::IntegerComputations
                    | Self::FloatingPointComputations
            }
        }
    }
}

impl FromStr for Profile {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "adaptive_ri" => Ok(Self::AdaptiveRI),
            "adaptive_rif" => Ok(Self::AdaptiveRIF),
            "base" => Ok(Self::Base),
            "unrestricted" => Ok(Self::Unrestricted),
            _ => Err(()),
        }
    }
}
