// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use thiserror::Error;

use crate::{model::Module, qir, validation::QirProfileError};

pub(super) const DEFAULT_MAX_FUNCS: usize = 1;
pub(super) const DEFAULT_MAX_BLOCKS_PER_FUNC: usize = 6;
pub(super) const DEFAULT_MAX_INSTRS_PER_BLOCK: usize = 12;
pub(super) const BASE_V1_BLOCK_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QirProfilePreset {
    BaseV1,
    AdaptiveV1,
    AdaptiveV2,
    BareRoundtrip,
}

impl QirProfilePreset {
    #[must_use]
    pub fn to_qir_profile(self) -> Option<qir::QirProfile> {
        match self {
            Self::BaseV1 => Some(qir::QirProfile::BaseV1),
            Self::AdaptiveV1 => Some(qir::QirProfile::AdaptiveV1),
            Self::AdaptiveV2 => Some(qir::QirProfile::AdaptiveV2),
            Self::BareRoundtrip => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    #[default]
    Model,
    Text,
    Bitcode,
    RoundTripChecked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundTripKind {
    TextOnly,
    BitcodeOnly,
    TextAndBitcodeSinglePass,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QirSmithConfig {
    pub profile: QirProfilePreset,
    pub output_mode: OutputMode,
    pub roundtrip: Option<RoundTripKind>,
    pub max_funcs: usize,
    pub max_blocks_per_func: usize,
    pub max_instrs_per_block: usize,
    pub allow_phi: bool,
    pub allow_switch: bool,
    pub allow_memory_ops: bool,
    pub allow_typed_pointers: bool,
    pub bare_roundtrip_mode: bool,
}

impl Default for QirSmithConfig {
    fn default() -> Self {
        Self::for_profile(QirProfilePreset::AdaptiveV2)
    }
}

impl QirSmithConfig {
    #[must_use]
    pub const fn for_profile(profile: QirProfilePreset) -> Self {
        let bare_roundtrip_mode = matches!(profile, QirProfilePreset::BareRoundtrip);
        let allow_typed_pointers = matches!(
            profile,
            QirProfilePreset::BaseV1 | QirProfilePreset::AdaptiveV1
        );
        let max_blocks_per_func = match profile {
            QirProfilePreset::BaseV1 => BASE_V1_BLOCK_COUNT,
            _ => DEFAULT_MAX_BLOCKS_PER_FUNC,
        };
        Self {
            profile,
            output_mode: OutputMode::Model,
            roundtrip: None,
            max_funcs: DEFAULT_MAX_FUNCS,
            max_blocks_per_func,
            max_instrs_per_block: DEFAULT_MAX_INSTRS_PER_BLOCK,
            allow_phi: false,
            allow_switch: false,
            allow_memory_ops: false,
            allow_typed_pointers,
            bare_roundtrip_mode,
        }
    }

    #[must_use]
    pub fn sanitize(&self) -> EffectiveConfig {
        let profile = if self.bare_roundtrip_mode
            || matches!(self.profile, QirProfilePreset::BareRoundtrip)
        {
            QirProfilePreset::BareRoundtrip
        } else {
            self.profile
        };

        let defaults = Self::for_profile(profile);
        let output_mode = self.output_mode;
        let roundtrip = if matches!(output_mode, OutputMode::RoundTripChecked) {
            let default_kind = if matches!(
                profile,
                QirProfilePreset::BaseV1 | QirProfilePreset::AdaptiveV1
            ) {
                RoundTripKind::TextOnly
            } else {
                RoundTripKind::TextAndBitcodeSinglePass
            };
            Some(self.roundtrip.unwrap_or(default_kind))
        } else {
            None
        };

        EffectiveConfig {
            profile,
            output_mode,
            roundtrip,
            max_funcs: super::sanitize_count(self.max_funcs, defaults.max_funcs),
            max_blocks_per_func: super::sanitize_count(
                self.max_blocks_per_func,
                defaults.max_blocks_per_func,
            ),
            max_instrs_per_block: super::sanitize_count(
                self.max_instrs_per_block,
                defaults.max_instrs_per_block,
            ),
            allow_phi: self.allow_phi && !matches!(output_mode, OutputMode::RoundTripChecked),
            allow_switch: self.allow_switch && !matches!(output_mode, OutputMode::RoundTripChecked),
            allow_memory_ops: self.allow_memory_ops
                && !matches!(output_mode, OutputMode::RoundTripChecked),
            allow_typed_pointers: defaults.allow_typed_pointers,
            bare_roundtrip_mode: defaults.bare_roundtrip_mode,
        }
    }

    pub(super) fn with_output_mode(&self, output_mode: OutputMode) -> Self {
        let mut config = self.clone();
        config.output_mode = output_mode;
        config
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveConfig {
    pub profile: QirProfilePreset,
    pub output_mode: OutputMode,
    pub roundtrip: Option<RoundTripKind>,
    pub max_funcs: usize,
    pub max_blocks_per_func: usize,
    pub max_instrs_per_block: usize,
    pub allow_phi: bool,
    pub allow_switch: bool,
    pub allow_memory_ops: bool,
    pub allow_typed_pointers: bool,
    pub bare_roundtrip_mode: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedArtifact {
    pub effective_config: EffectiveConfig,
    pub module: Module,
    pub text: Option<String>,
    pub bitcode: Option<Vec<u8>>,
}

#[derive(Debug, Error, Diagnostic, Clone, PartialEq, Eq)]
pub enum QirSmithError {
    #[error(
        "qir_smith generation is not implemented yet for profile {profile:?} in {output_mode:?} mode"
    )]
    #[diagnostic(code("Qsc.Llvm.QirSmith.GenerationNotImplemented"))]
    GenerationNotImplemented {
        profile: QirProfilePreset,
        output_mode: OutputMode,
    },

    #[error("qir_smith generated module is outside the supported v1 checked subset: {0}")]
    #[diagnostic(code("Qsc.Llvm.QirSmith.ModelGenerationFailed"))]
    ModelGeneration(String),

    #[error("qir_smith text roundtrip failed: {0}")]
    #[diagnostic(code("Qsc.Llvm.QirSmith.TextRoundTripFailed"))]
    TextRoundTrip(String),

    #[error("qir_smith bitcode roundtrip failed: {0}")]
    #[diagnostic(code("Qsc.Llvm.QirSmith.BitcodeRoundTripFailed"))]
    BitcodeRoundTrip(String),

    #[error(transparent)]
    #[diagnostic(transparent)]
    ProfileViolation(QirProfileError),
}
