// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use bitflags::bitflags;

bitflags! {
    /// These flags describe the capabilities of the target execution environment. They are used to determine which language features we can
    /// emit into generated code for that target, and correlate to which design-time errors are surfaced by the capabilities check pass.
    /// Note that the flags are in increasing order of capability and are expected to be largely additive. The code uses this "well-ordered"
    /// property to perform some checks using comparison operators, inaddition to the user bitwise membership checks offered by bitflags.
    /// Empty bitflags (0) corresponds to a "Base profile" target where no branching or classical computations are supported.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct TargetCapabilityFlags: u32 {
        /// Supports forward branching based on measurement results and reuse of qubits after measurement.
        const Adaptive = 0b0000_0001;
        /// Supports classical computations on integers (e.g. addition, multiplication).
        const IntegerComputations = 0b0000_0010;
        /// Supports classical computations on floating point numbers (e.g. addition, multiplication).
        const FloatingPointComputations = 0b0000_0100;
        /// Supports backward branching based on measurement results aka loops.
        const BackwardsBranching = 0b0000_1000;
        /// Supports statically sized arrays (i.e. array literals and array indexing with non-constant indices).
        const StaticSizedArrays = 0b0001_0000;
        /// Catch-all for high level language constructs not covered by other flags. New flags should be added above this one,
        /// such that this flag is reserved for the "all capabilities" targets that can run anything the langauge can express.
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

use std::str::FromStr;

/// The profile of the target environment, which formalizes the combined set of capabilities that target supports.
/// Most user-facing APIs work in terms of profiles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    /// Corresponds to a target with no limitations on supported language features.
    Unrestricted,
    /// Corresponds to a target with support only for gate operations on qubits with all measurements at the end of the program..
    Base,
    /// Corresponds to a target with support for forward branching, qubit reuse, and integer computations.
    AdaptiveRI,
    /// Corresponds to a target with support for forward branching, qubit reuse, integer computations, and floating point computations.
    AdaptiveRIF,
    /// Corresponds to a target with support for forward branching, qubit reuse, integer computations, floating point computations, loops, and static sized arrays.
    AdaptiveRIFLA,
}

impl Profile {
    #[must_use]
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Unrestricted => "Unrestricted",
            Self::Base => "Base",
            Self::AdaptiveRI => "Adaptive_RI",
            Self::AdaptiveRIF => "Adaptive_RIF",
            Self::AdaptiveRIFLA => "Adaptive_RIFLA",
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
            Profile::AdaptiveRIFLA => {
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
            "adaptive_rifla" => Ok(Self::AdaptiveRIFLA),
            "base" => Ok(Self::Base),
            "unrestricted" => Ok(Self::Unrestricted),
            _ => Err(()),
        }
    }
}
