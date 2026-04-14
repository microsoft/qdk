// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::time::{SystemTime, UNIX_EPOCH};

allocator::assign_global!();

use fuzz::qir_seed_bank::{
    QirSeedInput, default_qir_corpus_dir, generate_checked_seed_artifact, load_seed_inputs,
};
use qsc_llvm::{GeneratedArtifact, write_module_to_string};

const FAST_TOOLCHAINS: [u8; 4] = [14, 15, 16, 21];
const HOMEBREW_OPT_PREFIXES: [&str; 2] = ["/opt/homebrew/opt", "/usr/local/opt"];
const MIN_LLVM_VERSION: u8 = 14;
const MAX_LLVM_VERSION: u8 = 21;

#[derive(Debug)]
struct Options {
    toolchains: Vec<LlvmToolchain>,
    corpus_dir: PathBuf,
    output_dir: PathBuf,
}

#[derive(Debug)]
struct ReplaySummary {
    exported_artifacts: usize,
    replay_steps: usize,
    output_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LlvmToolchain {
    version: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PointerMode {
    Typed,
    Opaque,
}

impl PointerMode {
    fn from_artifact(artifact: &GeneratedArtifact) -> Self {
        if artifact.effective_config.allow_typed_pointers {
            Self::Typed
        } else {
            Self::Opaque
        }
    }

    const fn extra_args(self, version: u8) -> &'static [&'static str] {
        match (version, self) {
            (14, Self::Opaque) => &["-opaque-pointers"],
            _ => &[],
        }
    }
}

impl LlvmToolchain {
    fn tool_path(self, tool: &str) -> Result<PathBuf, String> {
        let mut candidates = Vec::new();

        if let Some(prefix) = env::var_os("HOMEBREW_PREFIX") {
            let path = PathBuf::from(prefix)
                .join("opt")
                .join(format!("llvm@{}", self.version))
                .join("bin")
                .join(tool);
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }

        for prefix in HOMEBREW_OPT_PREFIXES {
            let path = PathBuf::from(prefix)
                .join(format!("llvm@{}", self.version))
                .join("bin")
                .join(tool);
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }

        if let Some(path) = candidates.iter().find(|path| path.exists()) {
            Ok(path.clone())
        } else {
            Err(format!(
                "unable to find llvm@{} {} under any known Homebrew prefix: {}",
                self.version,
                tool,
                candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    }

    fn command(self, tool: &str, pointer_mode: PointerMode) -> Result<Command, String> {
        let mut command = Command::new(self.tool_path(tool)?);
        for arg in pointer_mode.extra_args(self.version) {
            command.arg(arg);
        }
        Ok(command)
    }

    fn ensure_available(self) -> Result<(), String> {
        for tool in ["llvm-as", "llvm-dis", "opt"] {
            let output = Command::new(self.tool_path(tool)?)
                .arg("--version")
                .output()
                .map_err(|error| {
                    format!(
                        "failed to run llvm@{} {} --version: {error}",
                        self.version, tool
                    )
                })?;
            if !output.status.success() {
                return Err(format!(
                    "llvm@{} {} --version failed: {}",
                    self.version,
                    tool,
                    summarize_output(&output)
                ));
            }
        }

        Ok(())
    }

    fn verify_bitcode(self, pointer_mode: PointerMode, bitcode_path: &Path) -> Result<(), String> {
        let output = self
            .command("opt", pointer_mode)?
            .arg("-passes=verify")
            .arg(bitcode_path)
            .arg("-disable-output")
            .output()
            .map_err(|error| {
                format!(
                    "failed to run llvm@{} opt on {}: {error}",
                    self.version,
                    bitcode_path.display()
                )
            })?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "llvm@{} opt verify failed for {}: {}",
                self.version,
                bitcode_path.display(),
                summarize_output(&output)
            ))
        }
    }

    fn disassemble_bitcode(
        self,
        pointer_mode: PointerMode,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), String> {
        let output = self
            .command("llvm-dis", pointer_mode)?
            .arg(input_path)
            .arg("-o")
            .arg(output_path)
            .output()
            .map_err(|error| {
                format!(
                    "failed to run llvm@{} llvm-dis on {}: {error}",
                    self.version,
                    input_path.display()
                )
            })?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "llvm@{} llvm-dis failed for {}: {}",
                self.version,
                input_path.display(),
                summarize_output(&output)
            ))
        }
    }

    fn assemble_text(
        self,
        pointer_mode: PointerMode,
        input_path: &Path,
        output_path: &Path,
    ) -> Result<(), String> {
        let output = self
            .command("llvm-as", pointer_mode)?
            .arg(input_path)
            .arg("-o")
            .arg(output_path)
            .output()
            .map_err(|error| {
                format!(
                    "failed to run llvm@{} llvm-as on {}: {error}",
                    self.version,
                    input_path.display()
                )
            })?;

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "llvm@{} llvm-as failed for {}: {}",
                self.version,
                input_path.display(),
                summarize_output(&output)
            ))
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(Some(summary)) => {
            println!(
                "exported {} valid artifacts across {} replay steps into {}",
                summary.exported_artifacts,
                summary.replay_steps,
                summary.output_dir.display()
            );
            ExitCode::SUCCESS
        }
        Ok(None) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("qir_matrix failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<Option<ReplaySummary>, String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!("{}", usage());
        return Ok(None);
    }

    let options = Options::parse(&args)?;
    for toolchain in &options.toolchains {
        toolchain.ensure_available()?;
    }

    fs::create_dir_all(&options.output_dir)
        .map_err(|error| format!("create {}: {error}", options.output_dir.display()))?;

    let seeds = load_seed_inputs(&options.corpus_dir)?;
    run_replay_matrix(&options, &seeds).map(Some)
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut toolchains = parse_toolchains(
            &FAST_TOOLCHAINS
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(","),
        )?;
        let mut corpus_dir = default_qir_corpus_dir();
        let mut output_dir = default_output_dir();

        let mut index = 0;
        while index < args.len() {
            match args[index].as_str() {
                "--toolchains" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "missing value for --toolchains".to_string())?;
                    toolchains = parse_toolchains(value)?;
                }
                "--corpus-dir" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "missing value for --corpus-dir".to_string())?;
                    corpus_dir = PathBuf::from(value);
                }
                "--output-dir" => {
                    index += 1;
                    let value = args
                        .get(index)
                        .ok_or_else(|| "missing value for --output-dir".to_string())?;
                    output_dir = PathBuf::from(value);
                }
                other => {
                    return Err(format!("unrecognized argument {other:?}\n\n{}", usage()));
                }
            }
            index += 1;
        }

        Ok(Self {
            toolchains,
            corpus_dir,
            output_dir,
        })
    }
}

fn run_replay_matrix(options: &Options, seeds: &[QirSeedInput]) -> Result<ReplaySummary, String> {
    let mut exported_artifacts = 0;
    let mut replay_steps = 0;

    for seed in seeds {
        let artifact =
            generate_checked_seed_artifact(seed.profile, &seed.bytes).map_err(|error| {
                format!(
                    "checked artifact generation failed for {} ({}): {error}",
                    seed.name,
                    seed.path.display()
                )
            })?;
        let pointer_mode = PointerMode::from_artifact(&artifact);
        let text = artifact
            .text
            .clone()
            .unwrap_or_else(|| write_module_to_string(&artifact.module));
        let artifact_path = options.output_dir.join(format!("{}.ll", seed.name));
        fs::write(&artifact_path, text)
            .map_err(|error| format!("write {}: {error}", artifact_path.display()))?;
        exported_artifacts += 1;

        for toolchain in &options.toolchains {
            replay_artifact(
                *toolchain,
                pointer_mode,
                &seed.name,
                &artifact_path,
                &options.output_dir,
            )
            .map_err(|error| format!("seed {}: {error}", seed.name))?;
            replay_steps += 1;
        }
    }

    if exported_artifacts == 0 {
        return Err(format!(
            "no checked-valid seed artifacts were found under {}",
            options.corpus_dir.display()
        ));
    }

    Ok(ReplaySummary {
        exported_artifacts,
        replay_steps,
        output_dir: options.output_dir.clone(),
    })
}

fn replay_artifact(
    toolchain: LlvmToolchain,
    pointer_mode: PointerMode,
    seed_name: &str,
    artifact_path: &Path,
    output_dir: &Path,
) -> Result<(), String> {
    let assembled_bitcode_path =
        output_dir.join(format!("{seed_name}.llvm{}.bc", toolchain.version));
    let disassembly_path = output_dir.join(format!("{seed_name}.llvm{}.ll", toolchain.version));
    let roundtrip_bitcode_path =
        output_dir.join(format!("{seed_name}.llvm{}.rt.bc", toolchain.version));

    toolchain.assemble_text(pointer_mode, artifact_path, &assembled_bitcode_path)?;
    toolchain.verify_bitcode(pointer_mode, &assembled_bitcode_path)?;
    toolchain.disassemble_bitcode(pointer_mode, &assembled_bitcode_path, &disassembly_path)?;
    toolchain.assemble_text(pointer_mode, &disassembly_path, &roundtrip_bitcode_path)?;
    toolchain.verify_bitcode(pointer_mode, &roundtrip_bitcode_path)
}

fn parse_toolchains(csv: &str) -> Result<Vec<LlvmToolchain>, String> {
    let mut toolchains = Vec::new();
    for raw_version in csv.split(',') {
        let version_text = raw_version.trim();
        if version_text.is_empty() {
            continue;
        }

        let version = version_text.parse::<u8>().map_err(|error| {
            format!("failed to parse LLVM toolchain version {version_text:?}: {error}")
        })?;
        if !(MIN_LLVM_VERSION..=MAX_LLVM_VERSION).contains(&version) {
            return Err(format!(
                "LLVM toolchain version {version} is out of range; expected {MIN_LLVM_VERSION} through {MAX_LLVM_VERSION}"
            ));
        }
        if !toolchains.contains(&LlvmToolchain { version }) {
            toolchains.push(LlvmToolchain { version });
        }
    }

    if toolchains.is_empty() {
        return Err("no LLVM toolchains requested".to_string());
    }

    Ok(toolchains)
}

fn default_output_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!("qir-matrix-{}-{nanos}", std::process::id()))
}

fn summarize_output(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        "command produced no output".to_string()
    } else {
        stdout
    }
}

fn usage() -> String {
    format!(
        "Usage: cargo run -p fuzz --bin qir_matrix -- [--toolchains 14,15,16,21] [--corpus-dir PATH] [--output-dir PATH]\n\nDefault corpus directory: {}\nFast matrix default: 14,15,16,21\nFull matrix example: --toolchains 14,15,16,17,18,19,20,21",
        default_qir_corpus_dir().display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_toolchains_accepts_fast_and_full_matrix() {
        let fast = parse_toolchains("14,15,16,21").expect("fast matrix should parse");
        assert_eq!(
            fast.iter()
                .map(|toolchain| toolchain.version)
                .collect::<Vec<_>>(),
            vec![14, 15, 16, 21]
        );

        let full = parse_toolchains("14,15,16,17,18,19,20,21").expect("full matrix should parse");
        assert_eq!(
            full.iter()
                .map(|toolchain| toolchain.version)
                .collect::<Vec<_>>(),
            vec![14, 15, 16, 17, 18, 19, 20, 21]
        );
    }

    #[test]
    fn parse_toolchains_rejects_versions_outside_supported_range() {
        assert!(parse_toolchains("13").is_err());
        assert!(parse_toolchains("22").is_err());
    }
}
