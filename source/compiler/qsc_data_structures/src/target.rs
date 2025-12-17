// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TargetCapabilityFlags: u32 {
        const Adaptive = 0b0000_0001;
        const IntegerComputations = 0b0000_0010;
        const FloatingPointComputations = 0b0000_0100;
        const BackwardsBranching = 0b0000_1000;
        const StaticSizedArrays = 0b0001_0000;
        const QubitVariables = 0b0010_0000;
        const HigherLevelConstructs = 0b1000_0000;
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
            "StaticSizedArrays" => Ok(TargetCapabilityFlags::StaticSizedArrays),
            "QubitVariables" => Ok(TargetCapabilityFlags::QubitVariables),
            "HigherLevelConstructs" => Ok(TargetCapabilityFlags::HigherLevelConstructs),
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

impl TargetCapabilityFlags {
    /// Any capability set that includes `BackwardsBranching` or greater is considered advanced.
    #[must_use]
    pub fn is_advanced(&self) -> bool {
        self.bits() >= TargetCapabilityFlags::BackwardsBranching.bits()
    }
}

use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    Unrestricted,
    Base,
    AdaptiveRI,
    AdaptiveRIF,
    Advanced,
    Advanced2,
}

impl Profile {
    #[must_use]
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Unrestricted => "Unrestricted",
            Self::Base => "Base",
            Self::AdaptiveRI => "Adaptive_RI",
            Self::AdaptiveRIF => "Adaptive_RIF",
            Self::Advanced => "Advanced",
            Self::Advanced2 => "Advanced2",
        }
    }
}

impl From<Profile> for TargetCapabilityFlags {
    fn from(value: Profile) -> Self {
        match value {
            Profile::Unrestricted => Self::all(),
            Profile::Base => Self::empty(),
            Profile::AdaptiveRI => Self::Adaptive | Self::IntegerComputations,
            Profile::AdaptiveRIF => {
                Self::Adaptive | Self::IntegerComputations | Self::FloatingPointComputations
            }
            Profile::Advanced => {
                Self::Adaptive
                    | Self::IntegerComputations
                    | Self::FloatingPointComputations
                    | Self::BackwardsBranching
            }
            Profile::Advanced2 => {
                Self::Adaptive
                    | Self::IntegerComputations
                    | Self::FloatingPointComputations
                    | Self::BackwardsBranching
                    | Self::StaticSizedArrays
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
            "advanced" => Ok(Self::Advanced),
            "advanced2" => Ok(Self::Advanced2),
            _ => Err(()),
        }
    }
}
