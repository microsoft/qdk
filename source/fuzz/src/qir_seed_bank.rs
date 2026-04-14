// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fs;
use std::path::{Path, PathBuf};

use qsc_llvm::fuzz::mutation::{
    MutationKind, SeedMutator, dispatch_mutation_family, mutation_selector,
    validate_mutated_module, validate_seed_artifact,
};
use qsc_llvm::fuzz::qir_mutations::mutate_adaptive_v1_typed_pointer_seed;
use qsc_llvm::{
    GeneratedArtifact, Module, QirProfilePreset, QirSmithConfig, QirSmithError,
    generate_checked_from_bytes,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QirSeedInput {
    pub name: String,
    pub profile: QirProfilePreset,
    pub bytes: Vec<u8>,
    pub path: PathBuf,
}

#[must_use]
pub fn default_qir_corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus/qir")
}

pub fn compile_seed_profile(profile: QirProfilePreset, profile_name: &str, data: &[u8]) {
    let artifact = generate_seed_artifact(profile, profile_name, data);
    validate_seed_artifact(&artifact);
}

pub fn compile(data: &[u8]) {
    compile_seed_profile(QirProfilePreset::AdaptiveV2, "AdaptiveV2", data);
}

pub fn compile_base_v1(data: &[u8]) {
    compile_seed_profile(QirProfilePreset::BaseV1, "BaseV1", data);
}

fn compile_mutated_profile(
    profile: QirProfilePreset,
    profile_name: &str,
    data: &[u8],
    mutator: SeedMutator,
) {
    let artifact = generate_seed_artifact(profile, profile_name, data);
    validate_seed_artifact(&artifact);

    let mutated = mutator(&artifact.module, data);
    validate_mutated_module(&mutated);
}

fn mutate_adaptive_v2_seed(seed: &Module, data: &[u8]) -> Module {
    let mut mutated = seed.clone();
    dispatch_mutation_family(
        &mut mutated,
        MutationKind::from_data(data),
        mutation_selector(data, 1),
    );
    mutated
}

fn mutate_typed_pointer_seed(seed: &Module, data: &[u8]) -> Module {
    mutate_adaptive_v1_typed_pointer_seed(seed, mutation_selector(data, 0))
}

pub fn compile_mutated_adaptive_v1(data: &[u8]) {
    compile_mutated_profile(
        QirProfilePreset::AdaptiveV1,
        "AdaptiveV1",
        data,
        mutate_typed_pointer_seed,
    );
}

pub fn compile_mutated_adaptive_v2(data: &[u8]) {
    compile_mutated_profile(
        QirProfilePreset::AdaptiveV2,
        "AdaptiveV2",
        data,
        mutate_adaptive_v2_seed,
    );
}

fn generate_seed_artifact(
    profile: QirProfilePreset,
    profile_name: &str,
    data: &[u8],
) -> GeneratedArtifact {
    generate_checked_seed_artifact(profile, data)
        .unwrap_or_else(|err| panic!("qir_smith {profile_name} checked generation failed: {err}"))
}

pub fn load_seed_inputs(corpus_dir: &Path) -> Result<Vec<QirSeedInput>, String> {
    let mut entries = fs::read_dir(corpus_dir)
        .map_err(|error| format!("read {}: {error}", corpus_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read {}: {error}", corpus_dir.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);

    let mut seeds = Vec::new();
    for entry in entries {
        if !entry
            .file_type()
            .map_err(|error| format!("inspect {}: {error}", entry.path().display()))?
            .is_file()
        {
            continue;
        }

        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if path.extension().and_then(|value| value.to_str()) != Some("seed") {
            continue;
        }

        let Some(profile) = profile_from_seed_file_name(file_name) else {
            return Err(format!(
                "seed file {} must start with base-v1-, adaptive-v1-, adaptive-v2-, or bare-roundtrip-",
                path.display()
            ));
        };

        let bytes = fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
        if bytes.is_empty() {
            return Err(format!("seed file {} is empty", path.display()));
        }

        let name = file_name.trim_end_matches(".seed").to_string();
        seeds.push(QirSeedInput {
            name,
            profile,
            bytes,
            path,
        });
    }

    if seeds.is_empty() {
        return Err(format!(
            "no .seed inputs found under {}",
            corpus_dir.display()
        ));
    }

    Ok(seeds)
}

pub fn generate_checked_seed_artifact(
    profile: QirProfilePreset,
    bytes: &[u8],
) -> Result<GeneratedArtifact, QirSmithError> {
    let config = QirSmithConfig::for_profile(profile);
    generate_checked_from_bytes(&config, bytes)
}

fn profile_from_seed_file_name(file_name: &str) -> Option<QirProfilePreset> {
    if file_name.starts_with("base-v1-") {
        Some(QirProfilePreset::BaseV1)
    } else if file_name.starts_with("adaptive-v1-") {
        Some(QirProfilePreset::AdaptiveV1)
    } else if file_name.starts_with("adaptive-v2-") {
        Some(QirProfilePreset::AdaptiveV2)
    } else if file_name.starts_with("bare-roundtrip-") {
        Some(QirProfilePreset::BareRoundtrip)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsc_llvm::validate_qir_profile;

    const ADAPTIVE_V1_TYPED_FIXTURE: &[u8] = include_bytes!("../corpus/qir/adaptive-v1-typed.seed");

    #[test]
    fn profile_prefixes_map_to_expected_qir_profiles() {
        assert_eq!(
            profile_from_seed_file_name("base-v1-smoke.seed"),
            Some(QirProfilePreset::BaseV1)
        );
        assert_eq!(
            profile_from_seed_file_name("adaptive-v1-smoke.seed"),
            Some(QirProfilePreset::AdaptiveV1)
        );
        assert_eq!(
            profile_from_seed_file_name("adaptive-v2-smoke.seed"),
            Some(QirProfilePreset::AdaptiveV2)
        );
        assert_eq!(
            profile_from_seed_file_name("bare-roundtrip-smoke.seed"),
            Some(QirProfilePreset::BareRoundtrip)
        );
        assert_eq!(profile_from_seed_file_name("unexpected.seed"), None);
    }

    #[test]
    fn checked_seed_artifact_emits_text_for_typed_profiles() {
        let seed_bytes: Vec<u8> = (0_u8..=127).collect();
        let artifact = generate_checked_seed_artifact(QirProfilePreset::AdaptiveV1, &seed_bytes)
            .expect("typed replay artifact generation should succeed");

        assert!(artifact.text.is_some());
        assert!(artifact.bitcode.is_none());
    }

    #[test]
    fn checked_seed_artifact_replays_base_v1_fixture() {
        let seed_bytes =
            b"base-v1|entry|0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ|0001";
        let artifact = generate_checked_seed_artifact(QirProfilePreset::BaseV1, seed_bytes)
            .expect("BaseV1 replay artifact generation should succeed");

        assert!(artifact.text.is_some());
        assert!(artifact.bitcode.is_none());
        assert!(
            validate_qir_profile(&artifact.module).errors.is_empty(),
            "BaseV1 replay artifact should satisfy the QIR profile"
        );
    }

    #[test]
    fn adaptive_v2_checked_generation_accepts_adaptive_v1_typed_fixture() {
        let artifact =
            generate_checked_seed_artifact(QirProfilePreset::AdaptiveV2, ADAPTIVE_V1_TYPED_FIXTURE)
                .expect("AdaptiveV2 generation should accept the adaptive-v1 typed corpus seed");

        assert!(artifact.text.is_some());
        assert!(artifact.bitcode.is_some());
        assert!(
            validate_qir_profile(&artifact.module).errors.is_empty(),
            "AdaptiveV2 artifact should satisfy the QIR profile for the typed fixture"
        );
    }
}
