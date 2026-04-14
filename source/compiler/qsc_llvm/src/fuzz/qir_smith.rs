// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod checked;
mod compare;
mod config;
mod generator;
mod io;
mod metadata;

#[cfg(test)]
mod tests;

use arbitrary::Unstructured;

use crate::model::Module;
#[cfg(test)]
use crate::model::{
    BasicBlock, BinOpKind, CastKind, Constant, Function, GlobalVariable, Instruction, IntPredicate,
    Linkage, MetadataValue, Operand, Param, Type,
};
#[cfg(test)]
use crate::qir;

pub use config::{
    EffectiveConfig, GeneratedArtifact, OutputMode, QirProfilePreset, QirSmithConfig,
    QirSmithError, RoundTripKind,
};

use checked::populate_checked_artifact;
#[cfg(test)]
use compare::{
    assert_bitcode_roundtrip_matches_supported_v1_subset, ensure_text_roundtrip_matches,
};
#[cfg(test)]
use config::{
    BASE_V1_BLOCK_COUNT, DEFAULT_MAX_BLOCKS_PER_FUNC, DEFAULT_MAX_FUNCS,
    DEFAULT_MAX_INSTRS_PER_BLOCK,
};
use generator::build_module_shell;
#[cfg(test)]
use generator::{
    GENERATED_TARGET_DATALAYOUT, GENERATED_TARGET_TRIPLE, QirGenState, ShellCounts, ShellPreset,
    StableNameAllocator,
};
use io::{emit_bitcode, emit_text};
#[cfg(test)]
use io::{parse_bitcode_roundtrip, parse_text_roundtrip};
#[cfg(test)]
use metadata::build_qdk_metadata;
use metadata::finalize_float_computations;

pub fn generate(
    config: &QirSmithConfig,
    bytes: &mut Unstructured<'_>,
) -> Result<GeneratedArtifact, QirSmithError> {
    let effective = config.sanitize();
    generate_artifact(&effective, bytes)
}

pub fn generate_from_bytes(
    config: &QirSmithConfig,
    bytes: &[u8],
) -> Result<GeneratedArtifact, QirSmithError> {
    with_unstructured_bytes(bytes, |unstructured| generate(config, unstructured))
}

pub fn generate_module(
    config: &QirSmithConfig,
    bytes: &mut Unstructured<'_>,
) -> Result<Module, QirSmithError> {
    let artifact = generate_for_mode(config, bytes, OutputMode::Model)?;
    Ok(artifact.module)
}

pub fn generate_module_from_bytes(
    config: &QirSmithConfig,
    bytes: &[u8],
) -> Result<Module, QirSmithError> {
    with_unstructured_bytes(bytes, |unstructured| generate_module(config, unstructured))
}

pub fn generate_text(
    config: &QirSmithConfig,
    bytes: &mut Unstructured<'_>,
) -> Result<String, QirSmithError> {
    let artifact = generate_for_mode(config, bytes, OutputMode::Text)?;
    Ok(artifact.text.unwrap_or_default())
}

pub fn generate_text_from_bytes(
    config: &QirSmithConfig,
    bytes: &[u8],
) -> Result<String, QirSmithError> {
    with_unstructured_bytes(bytes, |unstructured| generate_text(config, unstructured))
}

pub fn generate_bitcode(
    config: &QirSmithConfig,
    bytes: &mut Unstructured<'_>,
) -> Result<Vec<u8>, QirSmithError> {
    let artifact = generate_for_mode(config, bytes, OutputMode::Bitcode)?;
    Ok(artifact.bitcode.unwrap_or_default())
}

pub fn generate_bitcode_from_bytes(
    config: &QirSmithConfig,
    bytes: &[u8],
) -> Result<Vec<u8>, QirSmithError> {
    with_unstructured_bytes(bytes, |unstructured| generate_bitcode(config, unstructured))
}

pub fn generate_checked(
    config: &QirSmithConfig,
    bytes: &mut Unstructured<'_>,
) -> Result<GeneratedArtifact, QirSmithError> {
    generate_for_mode(config, bytes, OutputMode::RoundTripChecked)
}

pub fn generate_checked_from_bytes(
    config: &QirSmithConfig,
    bytes: &[u8],
) -> Result<GeneratedArtifact, QirSmithError> {
    with_unstructured_bytes(bytes, |unstructured| generate_checked(config, unstructured))
}

fn with_unstructured_bytes<T>(
    bytes: &[u8],
    generate: impl FnOnce(&mut Unstructured<'_>) -> Result<T, QirSmithError>,
) -> Result<T, QirSmithError> {
    let mut unstructured = Unstructured::new(bytes);
    generate(&mut unstructured)
}

fn generate_for_mode(
    config: &QirSmithConfig,
    bytes: &mut Unstructured<'_>,
    output_mode: OutputMode,
) -> Result<GeneratedArtifact, QirSmithError> {
    let config = config.with_output_mode(output_mode);
    generate(&config, bytes)
}

fn generate_artifact(
    effective: &EffectiveConfig,
    bytes: &mut Unstructured<'_>,
) -> Result<GeneratedArtifact, QirSmithError> {
    let mut artifact = build_generated_artifact(effective, bytes)?;
    populate_requested_outputs(&mut artifact)?;
    Ok(artifact)
}

fn build_generated_artifact(
    effective: &EffectiveConfig,
    bytes: &mut Unstructured<'_>,
) -> Result<GeneratedArtifact, QirSmithError> {
    let mut module = build_module_shell(effective, bytes);

    if matches!(
        effective.profile,
        QirProfilePreset::AdaptiveV1 | QirProfilePreset::AdaptiveV2
    ) {
        finalize_float_computations(&mut module);
    }

    Ok(GeneratedArtifact {
        effective_config: effective.clone(),
        module,
        text: None,
        bitcode: None,
    })
}

fn populate_requested_outputs(artifact: &mut GeneratedArtifact) -> Result<(), QirSmithError> {
    match artifact.effective_config.output_mode {
        OutputMode::Model => {}
        OutputMode::Text => {
            artifact.text = Some(emit_text(&artifact.module));
        }
        OutputMode::Bitcode => {
            artifact.bitcode = Some(emit_bitcode(&artifact.module)?);
        }
        OutputMode::RoundTripChecked => populate_checked_artifact(artifact)?,
    }

    Ok(())
}

const fn sanitize_count(value: usize, default: usize) -> usize {
    if value == 0 { default } else { value }
}
