// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Characterization tests recording which diagnostics reach an OpenQASM
//! `Compilation` today, and which pipeline stage produces them.
//!
//! Spec mode cuts diagnostics at stage 1 (semantic analysis) and detects
//! QDK incompatibility from stage 2 (lowering to Q# AST). Stage 3 (Q#
//! compilation) and stage 4 (FIR/capability passes) failures mean the QDK
//! tried and something else went wrong, so they must not select spec mode.
//! These fixtures pin one program per stage.

use super::Compilation;
use super::stage_one_diagnostics;
use crate::compilation::CompilationKind;
use crate::protocol::{EffectiveOpenQasmMode, OpenQasmMode};
use expect_test::{Expect, expect};
use qsc::{PackageType, compile, error::WithSource};
use std::sync::Arc;

/// A pulse-level program. Every construct here is rejected during lowering
/// (stage 2) because the QDK has no representation for it. `stdgates.inc` is
/// included deliberately so the fixture produces no stage-1 diagnostics and
/// isolates stage 2.
pub(super) const PULSE_LEVEL: &str = r#"OPENQASM 3.0;
include "stdgates.inc";
defcalgrammar "openpulse";
cal {
    extern frame drive_frame;
}
defcal x $0 {
    delay[100ns] drive_frame;
}
x $0;
"#;

/// A program the QDK compiles cleanly. No diagnostics from any stage.
pub(super) const QDK_COMPATIBLE: &str = r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
bit[2] c;
h q[0];
cx q[0], q[1];
c = measure q;
"#;

/// A program with a genuine OpenQASM error. Produced at stage 1, so it
/// survives into spec mode and does not select spec mode.
pub(super) const SPEC_ERROR: &str = r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[2] q;
h undefined_register[0];
"#;

/// A program that declares the Base profile then branches on a measurement
/// result, which Base does not permit. Clean through lowering, so the
/// diagnostic comes from stage 4.
pub(super) const CAPABILITY_VIOLATION: &str = r#"OPENQASM 3.0;
include "stdgates.inc";
#pragma qdk.qir.profile Base
qubit q;
bit c;
c = measure q;
if (c == 1) {
    x q;
}
"#;

/// A program with a malformed declaration. The parser reports two distinct
/// errors for it, neither repeated. This fixture pins that count so a later
/// change that collects stage-1 errors a second time shows up as duplication.
pub(super) const SYNTAX_ERROR: &str = r#"OPENQASM 3.0;
include "stdgates.inc";
qubit[2 q;
"#;

/// A parent whose only error lives in an included file, used to prove an
/// include's diagnostic is reported once rather than once per stage.
pub(super) const INCLUDE_PARENT: &str = r#"OPENQASM 3.0;
include "stdgates.inc";
include "broken.inc";
"#;

pub(super) const INCLUDE_CHILD: &str = "qubit[2 q;\n";

pub(super) fn compile(source: &str) -> Compilation {
    compile_sources(&[("<source>", source)])
}

pub(super) fn compile_sources(sources: &[(&str, &str)]) -> Compilation {
    compile_sources_in_mode(sources, OpenQasmMode::Auto)
}

pub(super) fn compile_in_mode(source: &str, mode: OpenQasmMode) -> Compilation {
    compile_sources_in_mode(&[("<source>", source)], mode)
}

pub(super) fn compile_sources_in_mode(sources: &[(&str, &str)], mode: OpenQasmMode) -> Compilation {
    Compilation::new_qasm(
        PackageType::Lib,
        sources
            .iter()
            .map(|(name, source)| (Arc::from(*name), Arc::from(*source)))
            .collect(),
        vec![],
        &Arc::from("test project"),
        mode,
    )
}

/// The mode a fixture resolved to, for comparison in mode tests.
fn effective_mode(compilation: &Compilation) -> EffectiveOpenQasmMode {
    match compilation.kind {
        CompilationKind::OpenQASM { effective_mode, .. } => effective_mode,
        _ => panic!("expected an OpenQASM compilation"),
    }
}

/// Renders code, message, and label spans, so a comparison catches a
/// conversion that keeps the code but loses the source attribution.
fn render(errors: &[WithSource<compile::ErrorKind>]) -> Vec<String> {
    errors
        .iter()
        .map(|e| {
            let code = miette::Diagnostic::code(e)
                .map_or_else(|| "<none>".to_string(), |code| code.to_string());
            let labels = miette::Diagnostic::labels(e).map_or_else(String::new, |labels| {
                labels
                    .map(|l| format!("{}..{}", l.offset(), l.offset() + l.len()))
                    .collect::<Vec<_>>()
                    .join(",")
            });
            format!("{code} | {e} | [{labels}]")
        })
        .collect()
}

/// Renders each published diagnostic as its error code, or its message when
/// the diagnostic carries no code. Pinned to qdk mode so these keep recording
/// per-stage output rather than the mode's effect on it.
fn check_diagnostics(source: &str, expect: &Expect) {
    let compilation = compile_in_mode(source, OpenQasmMode::Qdk);
    let actual = compilation
        .compile_errors
        .iter()
        .map(|e| miette::Diagnostic::code(e).map_or_else(|| e.to_string(), |code| code.to_string()))
        .collect::<Vec<_>>();
    expect.assert_debug_eq(&actual);
}

#[test]
fn pulse_level_program_diagnostics() {
    check_diagnostics(
        PULSE_LEVEL,
        &expect![[r#"
            [
                "Qdk.Qasm.Compiler.NotSupported",
                "Qdk.Qasm.Compiler.NotSupported",
                "Qdk.Qasm.Compiler.NotSupported",
                "Qdk.Qasm.Compiler.NotSupported",
            ]
        "#]],
    );
}

#[test]
fn qdk_compatible_program_diagnostics() {
    check_diagnostics(
        QDK_COMPATIBLE,
        &expect![[r#"
            []
        "#]],
    );
}

#[test]
fn spec_error_program_diagnostics() {
    check_diagnostics(
        SPEC_ERROR,
        &expect![[r#"
            [
                "Qdk.Qasm.Lowerer.UndefinedSymbol",
                "Qdk.Qasm.Lowerer.CannotIndexType",
            ]
        "#]],
    );
}

#[test]
fn capability_violation_program_diagnostics() {
    check_diagnostics(
        CAPABILITY_VIOLATION,
        &expect![[r#"
            [
                "Qdk.Qsc.CapabilitiesCk.UseOfDynamicBool",
                "Qdk.Qsc.CapabilitiesCk.UseOfDynamicInt",
                "Qdk.Qsc.CapabilitiesCk.UseOfDynamicBool",
                "Qdk.Qsc.CapabilitiesCk.UseOfDynamicInt",
            ]
        "#]],
    );
}

#[test]
fn syntax_error_program_diagnostics() {
    check_diagnostics(
        SYNTAX_ERROR,
        &expect![[r#"
            [
                "Qdk.Qasm.Parser.Token",
                "Qdk.Qasm.Parser.Rule",
            ]
        "#]],
    );
}

/// When a program's only errors come from stage 1, the converted stage-1 set
/// and the set published today must be indistinguishable. This is what proves
/// the conversion preserves code, message, and source attribution.
#[test]
fn stage_one_conversion_matches_published_diagnostics() {
    for source in [SPEC_ERROR, SYNTAX_ERROR] {
        let sources = vec![(Arc::from("<source>"), Arc::from(source))];
        let res = qsc::openqasm::semantic::parse_sources(&sources);
        let converted = stage_one_diagnostics(&res);

        let published = compile(source);

        assert_eq!(render(&converted), render(&published.compile_errors));
        assert!(!converted.is_empty(), "fixture should produce diagnostics");
    }
}

/// Guards the `all_errors()` hazard: it would prepend every syntax error a
/// second time, which duplicates diagnostics and inflates the length that
/// `auto` detection compares against.
#[test]
fn syntax_errors_are_not_duplicated() {
    let sources = vec![(Arc::from("<source>"), Arc::from(SYNTAX_ERROR))];
    let res = qsc::openqasm::semantic::parse_sources(&sources);

    let from_field = stage_one_diagnostics(&res);
    let rendered = render(&from_field);
    let mut deduped = rendered.clone();
    deduped.sort_unstable();
    deduped.dedup();

    assert_eq!(rendered.len(), deduped.len(), "diagnostics were duplicated");
    assert_eq!(rendered.len(), res.errors.len());
    assert!(
        res.all_errors().len() > res.errors.len(),
        "all_errors() should be the larger set; if this fails the hazard is gone \
         and stage_one_diagnostics can be simplified"
    );
}

#[test]
fn syntax_error_in_included_file_reported_once() {
    let compilation =
        compile_sources(&[("<source>", INCLUDE_PARENT), ("broken.inc", INCLUDE_CHILD)]);

    let rendered = render(&compilation.compile_errors);
    let mut deduped = rendered.clone();
    deduped.sort_unstable();
    deduped.dedup();

    assert_eq!(rendered.len(), deduped.len(), "{rendered:#?}");
    expect![[r#"
        [
            "Qdk.Qasm.Parser.Token | expected `]`, found identifier | [69..70]",
            "Qdk.Qasm.Parser.Rule | expected identifier, found EOF | [72..72]",
        ]
    "#]]
    .assert_debug_eq(&rendered);
}

#[test]
fn auto_selects_spec_only_for_stage_two_failures() {
    assert_eq!(
        effective_mode(&compile(PULSE_LEVEL)),
        EffectiveOpenQasmMode::Spec
    );

    // Every other fixture must stay in qdk mode. A stage-4 capability violation
    // in particular is the QDK compiling the program and rejecting it for the
    // selected profile, which spec mode would hide rather than explain.
    for source in [QDK_COMPATIBLE, CAPABILITY_VIOLATION, SYNTAX_ERROR] {
        assert_eq!(
            effective_mode(&compile(source)),
            EffectiveOpenQasmMode::Qdk,
            "{source}"
        );
    }
}

/// The case that separates "your code is wrong" from "the QDK cannot compile
/// your code". A spec error is a stage-1 diagnostic, so it must not switch the
/// user into a mode where the QDK stops looking at their program.
#[test]
fn auto_keeps_qdk_mode_for_a_program_whose_only_error_is_a_spec_error() {
    let compilation = compile(SPEC_ERROR);

    assert!(!compilation.compile_errors.is_empty());
    assert_eq!(effective_mode(&compilation), EffectiveOpenQasmMode::Qdk);
}

#[test]
fn explicit_mode_ignores_detection() {
    // PULSE_LEVEL would auto-detect as spec, QDK_COMPATIBLE as qdk. Neither
    // detection result may override an explicit choice.
    for source in [PULSE_LEVEL, QDK_COMPATIBLE] {
        assert_eq!(
            effective_mode(&compile_in_mode(source, OpenQasmMode::Qdk)),
            EffectiveOpenQasmMode::Qdk,
            "{source}"
        );
        assert_eq!(
            effective_mode(&compile_in_mode(source, OpenQasmMode::Spec)),
            EffectiveOpenQasmMode::Spec,
            "{source}"
        );
    }
}

/// Resolution order in isolation, including the precedence a session override
/// will rely on once it has a writer.
#[test]
fn override_beats_configuration_and_clearing_restores_it() {
    let configured = OpenQasmMode::Qdk;
    let resolve = |session_override: Option<OpenQasmMode>, stage_two_appended| {
        super::resolve_openqasm_mode(session_override.unwrap_or(configured), stage_two_appended)
    };

    assert_eq!(
        resolve(Some(OpenQasmMode::Spec), false),
        EffectiveOpenQasmMode::Spec
    );
    assert_eq!(resolve(None, false), EffectiveOpenQasmMode::Qdk);
    assert_eq!(
        resolve(Some(OpenQasmMode::Auto), true),
        EffectiveOpenQasmMode::Spec
    );
}

/// The mode must be re-resolved on recompilation. Without this, changing the
/// setting recompiles with whatever mode the first compile happened to use and
/// the setting looks inert.
#[test]
fn recompile_adopts_the_new_mode() {
    let mut compilation = compile(QDK_COMPATIBLE);
    assert_eq!(effective_mode(&compilation), EffectiveOpenQasmMode::Qdk);

    compilation.recompile(
        PackageType::Lib,
        qsc::target::Profile::Unrestricted,
        qsc::LanguageFeatures::default(),
        &[],
        OpenQasmMode::Spec,
    );

    assert_eq!(effective_mode(&compilation), EffectiveOpenQasmMode::Spec);
}

#[test]
fn spec_mode_publishes_nothing_for_a_qdk_unsupported_program() {
    let compilation = compile_in_mode(PULSE_LEVEL, OpenQasmMode::Spec);

    assert_eq!(
        render(&compilation.compile_errors),
        Vec::<String>::new(),
        "spec mode must not report the QDK's inability to lower the program"
    );
}

#[test]
fn spec_mode_still_publishes_spec_errors_exactly_once() {
    for source in [SPEC_ERROR, SYNTAX_ERROR] {
        let spec = render(&compile_in_mode(source, OpenQasmMode::Spec).compile_errors);
        let qdk = render(&compile_in_mode(source, OpenQasmMode::Qdk).compile_errors);

        // These fixtures fail at stage 1, so both modes cut at the same place.
        assert_eq!(spec, qdk, "{source}");

        let mut deduped = spec.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(spec.len(), deduped.len(), "{source}");
    }
}

/// Spec mode must also drop stage-4 output, which `run_fir_passes` produces
/// after lowering has already succeeded.
#[test]
fn spec_mode_drops_capability_diagnostics() {
    assert!(
        !compile_in_mode(CAPABILITY_VIOLATION, OpenQasmMode::Qdk)
            .compile_errors
            .is_empty()
    );
    assert!(
        compile_in_mode(CAPABILITY_VIOLATION, OpenQasmMode::Spec)
            .compile_errors
            .is_empty()
    );
}

/// Project errors describe file resolution failures, not the QDK's view of the
/// program, so spec mode's cut must not touch them.
#[test]
fn spec_mode_keeps_project_errors() {
    let project_error = qsc::project::Error::FileSystem {
        about_path: "missing.inc".to_string(),
        error: "not found".to_string(),
    };

    let compilation = Compilation::new_qasm(
        PackageType::Lib,
        vec![(Arc::from("<source>"), Arc::from(PULSE_LEVEL))],
        vec![project_error],
        &Arc::from("test project"),
        OpenQasmMode::Spec,
    );

    assert!(compilation.compile_errors.is_empty());
    assert_eq!(compilation.project_errors.len(), 1);
}

/// `add_unnecessary_code_diagnostics` runs on every compilation and derives
/// grey-out ranges from dropped spans. It is only inert for OpenQASM if
/// lowering never drops any, which is what makes it safe to leave alone in
/// spec mode.
#[test]
fn openqasm_lowering_drops_no_spans() {
    for source in [
        PULSE_LEVEL,
        QDK_COMPATIBLE,
        SPEC_ERROR,
        CAPABILITY_VIOLATION,
    ] {
        for mode in [OpenQasmMode::Qdk, OpenQasmMode::Spec] {
            let compilation = compile_in_mode(source, mode);
            assert!(compilation.user_unit().dropped_spans.is_empty(), "{source}");
        }
    }
}

/// Spec mode is only safe while QDK-only rejections remain after semantic
/// analysis. Moving one into stage 1 would make spec mode publish it.
#[test]
fn qdk_only_constructs_do_not_produce_stage_one_diagnostics() {
    let sources = vec![(Arc::from("<source>"), Arc::from(PULSE_LEVEL))];
    let res = qsc::openqasm::semantic::parse_sources(&sources);

    assert!(
        stage_one_diagnostics(&res).is_empty(),
        "PULSE_LEVEL must remain clean through stage 1 for spec mode to hide \
         only QDK-only diagnostics"
    );
}
