// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LlvmCompatLaneKind {
    LegacyTyped,
    BridgeDualMode,
    OpaquePreferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LlvmCompatLane {
    pub(crate) version: u8,
    pub(crate) kind: LlvmCompatLaneKind,
}

impl LlvmCompatLane {
    pub(crate) const LLVM_14: Self = Self {
        version: 14,
        kind: LlvmCompatLaneKind::LegacyTyped,
    };
    pub(crate) const LLVM_15: Self = Self {
        version: 15,
        kind: LlvmCompatLaneKind::BridgeDualMode,
    };
    pub(crate) const LLVM_16: Self = Self {
        version: 16,
        kind: LlvmCompatLaneKind::OpaquePreferred,
    };
    pub(crate) const LLVM_21: Self = Self {
        version: 21,
        kind: LlvmCompatLaneKind::OpaquePreferred,
    };
    pub(crate) const FAST_MATRIX: [Self; 4] =
        [Self::LLVM_14, Self::LLVM_15, Self::LLVM_16, Self::LLVM_21];

    #[must_use]
    pub(crate) fn tool_path(self, tool: &str) -> PathBuf {
        PathBuf::from(format!(
            "/opt/homebrew/opt/llvm@{}/bin/{tool}",
            self.version
        ))
    }

    #[must_use]
    pub(crate) fn tool_command(self, tool: &str) -> Command {
        Command::new(self.tool_path(tool))
    }

    #[must_use]
    pub(crate) fn has_tool(self, tool: &str) -> bool {
        self.tool_command(tool)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    #[must_use]
    pub(crate) fn is_available(self) -> bool {
        ["llvm-as", "llvm-dis", "opt"]
            .into_iter()
            .all(|tool| self.has_tool(tool))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointerProbe {
    TypedText,
    OpaqueText,
}

impl PointerProbe {
    #[must_use]
    pub(crate) const fn tool_args(self, lane: LlvmCompatLane) -> &'static [&'static str] {
        match (lane.version, self) {
            (14, Self::OpaqueText) => &["-opaque-pointers"],
            _ => &[],
        }
    }
}

#[must_use]
pub(crate) fn available_fast_matrix_lanes() -> Vec<LlvmCompatLane> {
    LlvmCompatLane::FAST_MATRIX
        .into_iter()
        .filter(|lane| lane.is_available())
        .collect()
}

pub(crate) fn assemble_text_ir(
    lane: LlvmCompatLane,
    probe: PointerProbe,
    text: &str,
) -> Result<Vec<u8>, String> {
    let tmp_ll = unique_temp_path(&format!("qsc-llvm{}", lane.version), "ll");
    let tmp_bc = unique_temp_path(&format!("qsc-llvm{}", lane.version), "bc");

    std::fs::write(&tmp_ll, text)
        .map_err(|error| format!("write {}: {error}", tmp_ll.display()))?;

    let mut command = lane.tool_command("llvm-as");
    for arg in probe.tool_args(lane) {
        command.arg(arg);
    }
    let output = command
        .arg(&tmp_ll)
        .arg("-o")
        .arg(&tmp_bc)
        .output()
        .map_err(|error| format!("spawn llvm-as {}: {error}", lane.version))?;

    std::fs::remove_file(&tmp_ll).ok();

    if !output.status.success() {
        std::fs::remove_file(&tmp_bc).ok();
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }

    let bitcode =
        std::fs::read(&tmp_bc).map_err(|error| format!("read {}: {error}", tmp_bc.display()))?;
    std::fs::remove_file(&tmp_bc).ok();
    Ok(bitcode)
}

pub(crate) fn disassemble_bitcode(
    lane: LlvmCompatLane,
    probe: PointerProbe,
    bitcode: &[u8],
) -> Result<String, String> {
    let tmp_bc = unique_temp_path(&format!("qsc-llvm{}", lane.version), "bc");
    std::fs::write(&tmp_bc, bitcode)
        .map_err(|error| format!("write {}: {error}", tmp_bc.display()))?;

    let mut command = lane.tool_command("llvm-dis");
    for arg in probe.tool_args(lane) {
        command.arg(arg);
    }
    let output = command
        .arg(&tmp_bc)
        .arg("-o")
        .arg("-")
        .output()
        .map_err(|error| format!("spawn llvm-dis {}: {error}", lane.version))?;

    std::fs::remove_file(&tmp_bc).ok();

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }

    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

pub(crate) fn verify_bitcode(
    lane: LlvmCompatLane,
    probe: PointerProbe,
    bitcode: &[u8],
) -> Result<(), String> {
    let tmp_bc = unique_temp_path(&format!("qsc-llvm{}", lane.version), "bc");
    std::fs::write(&tmp_bc, bitcode)
        .map_err(|error| format!("write {}: {error}", tmp_bc.display()))?;

    let mut command = lane.tool_command("opt");
    for arg in probe.tool_args(lane) {
        command.arg(arg);
    }
    let output = command
        .arg("-passes=verify")
        .arg(&tmp_bc)
        .arg("-disable-output")
        .output()
        .map_err(|error| format!("spawn opt {}: {error}", lane.version))?;

    std::fs::remove_file(&tmp_bc).ok();

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

pub(crate) fn analyze_bitcode(lane: LlvmCompatLane, bitcode: &[u8]) -> Result<String, String> {
    let tmp_bc = unique_temp_path(&format!("qsc-llvm{}", lane.version), "bc");
    std::fs::write(&tmp_bc, bitcode)
        .map_err(|error| format!("write {}: {error}", tmp_bc.display()))?;

    let output = lane
        .tool_command("llvm-bcanalyzer")
        .arg("-dump")
        .arg("--disable-histogram")
        .arg(&tmp_bc)
        .output()
        .map_err(|error| format!("spawn llvm-bcanalyzer {}: {error}", lane.version))?;

    std::fs::remove_file(&tmp_bc).ok();

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }

    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

#[must_use]
fn unique_temp_path(prefix: &str, extension: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{nanos}-{counter}.{extension}",
        std::process::id()
    ))
}
