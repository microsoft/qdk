// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::codegen::qir::adaptive_fallback_eligible;
use qsc_fir::fir::PackageSpan;

pub(crate) const POSITIVE: &str = r#"
operation Count(q : Qubit) : Int {
    mutable n = 0;
    for _ in 1..3 {
        X(q);
        set n += 1;
    }
    n
}
@EntryPoint()
operation Main() : (Int, Result[]) {
    use q = Qubit();
    let n = Count(q);
    use qs = Qubit[n];
    let rs = MResetEachZ(qs);
    (n + (MResetZ(q) == One ? 10 | 0), rs)
}
"#;

const STRUCT_RETURN_SOURCE: &str = r#"
struct Counted { Count : Int }
operation Tally(q : Qubit) : Counted {
    mutable n = 0;
    for _ in 1..3 { X(q); set n += 1; }
    new Counted { Count = n }
}
operation Main() : (Int, Result) {
    use q = Qubit();
    let v = Tally(q);
    use qs = Qubit[v.Count];
    Reset(q);
    (Length(qs), MResetZ(qs[0]))
}
"#;

const FAILED_FALLBACK_NOTE: &str = "Adaptive_RIF fallback was also rejected; these diagnostics describe the requested Adaptive profile.";

fn assert_original_errors_with_fallback_note<T: miette::Diagnostic>(errors: &[T], original: &[T]) {
    assert!(!original.is_empty());
    assert_eq!(errors.len(), original.len());
    for (index, (error, original)) in errors.iter().zip(original).enumerate() {
        assert_eq!(error.to_string(), original.to_string());
        assert_eq!(
            error.code().map(|code| code.to_string()),
            original.code().map(|code| code.to_string())
        );
        assert_eq!(error.severity(), original.severity());
        assert_eq!(
            format!(
                "{:?}",
                error.labels().map(std::iter::Iterator::collect::<Vec<_>>)
            ),
            format!(
                "{:?}",
                original
                    .labels()
                    .map(std::iter::Iterator::collect::<Vec<_>>)
            )
        );
        let original_help = original.help().map(|help| help.to_string());
        let expected_help = if index == 0 {
            Some(original_help.map_or_else(
                || FAILED_FALLBACK_NOTE.to_string(),
                |help| format!("{help}\n\n{FAILED_FALLBACK_NOTE}"),
            ))
        } else {
            original_help
        };
        assert_eq!(error.help().map(|help| help.to_string()), expected_help);
    }
}

#[test]
fn adaptive_fallback_struct_return_allocation_matches_explicit_rif() {
    let result = detailed(STRUCT_RETURN_SOURCE);
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert_eq!(
        result.qir,
        compile_source_to_qir(STRUCT_RETURN_SOURCE, Profile::AdaptiveRIF.into())
    );
    assert!(matches!(
        result.warnings.as_slice(),
        [crate::interpret::Error::AdaptiveProfileFallback(_)]
    ));
}

#[test]
fn adaptive_fallback_struct_return_parameterized_fragment_remains_rejected() {
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    let mut cursor = Cursor::new(Vec::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    let errors = interpreter
        .eval_fragments(&mut receiver, STRUCT_RETURN_SOURCE)
        .expect_err("parameterized roots still require a separate admission strategy");
    assert!(errors.iter().any(|error| matches!(
        error,
        crate::interpret::Error::Pass(error)
            if matches!(error.error().capability_error(),
                Some(qsc_rca::errors::Error::UseOfDynamicUdt(_)))
    )));
}

#[test]
fn adaptive_fallback_fixed_quantum_loop_allocation_emits_rif_qir_from_source() {
    let qir = compile_source_to_qir(POSITIVE, Profile::Adaptive.into());
    assert_eq!(
        qir,
        compile_source_to_qir(POSITIVE, Profile::AdaptiveRIF.into())
    );
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body(").count(),
        3
    );
}

#[test]
fn adaptive_fallback_fixed_quantum_loop_allocation_emits_rif_qir_from_ast() {
    let (ast, sources) = parse_source_to_ast(POSITIVE);
    let capabilities = Profile::Adaptive.into();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let qir = get_qir_from_ast(&mut store, &[(std_id, None)], ast, sources, capabilities)
        .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors)));
    assert_eq!(qir.capabilities, Profile::AdaptiveRIF.into());
    assert!(matches!(
        qir.warnings.as_slice(),
        [crate::interpret::Error::AdaptiveProfileFallback(_)]
    ));
    assert_eq!(
        qir.qir,
        compile_source_to_qir(POSITIVE, Profile::AdaptiveRIF.into())
    );
}

fn strict_adaptive_errors(source: &str) -> Vec<crate::interpret::Error> {
    let caps = Profile::Adaptive.into();
    let (std_id, store) = crate::compile::package_store_with_stdlib(caps);
    get_rir(
        source_map_from_source(source),
        LanguageFeatures::default(),
        caps,
        store,
        &[(std_id, None)],
    )
    .expect_err("strict Adaptive analysis must reject this fixture")
}

#[test]
fn adaptive_fallback_mixed_array_and_exponent_errors_emit_rif_qir() {
    let source = POSITIVE.replace("(n +", "(2 ^ n +");
    let errors = strict_adaptive_errors(&source);
    assert!(errors.iter().any(|error| matches!(
        error,
        crate::interpret::Error::Pass(error)
            if matches!(error.error().capability_error(),
                Some(qsc_rca::errors::Error::UseOfDynamicallySizedArray(_)))
    )));
    assert!(errors.iter().any(|error| matches!(
        error,
        crate::interpret::Error::Pass(error)
            if matches!(error.error().capability_error(),
                Some(qsc_rca::errors::Error::UseOfDynamicExponent(_)))
    )));

    let result = detailed(&source);
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert!(matches!(
        result.warnings.as_slice(),
        [crate::interpret::Error::AdaptiveProfileFallback(_)]
    ));
    assert_eq!(
        result.qir,
        compile_source_to_qir(&source, Profile::AdaptiveRIF.into())
    );
    assert_eq!(
        result
            .qir
            .matches("call void @__quantum__qis__x__body(")
            .count(),
        3
    );
}

#[test]
fn adaptive_fallback_exponent_errors_without_array_errors_emit_rif_qir() {
    let source = POSITIVE
        .replace("Qubit[n]", "Qubit[3]")
        .replace("(n +", "(2 ^ n +");
    let errors = strict_adaptive_errors(&source);
    assert!(!errors.is_empty());
    assert!(errors.iter().all(|error| matches!(
        error,
        crate::interpret::Error::Pass(error)
            if matches!(error.error().capability_error(),
                Some(qsc_rca::errors::Error::UseOfDynamicExponent(_)))
    )));

    let result = detailed(&source);
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert!(matches!(
        result.warnings.as_slice(),
        [crate::interpret::Error::AdaptiveProfileFallback(_)]
    ));
    assert_eq!(
        result.qir,
        compile_source_to_qir(&source, Profile::AdaptiveRIF.into())
    );
}

#[test]
fn adaptive_fallback_mixed_measurement_dependent_errors_retain_original_diagnostics() {
    let source = POSITIVE
        .replace("let n = Count(q);", "let n = MResetZ(q) == One ? 3 | 2;")
        .replace("(n +", "(2 ^ n +");
    let original = strict_adaptive_errors(&source);
    assert!(original.iter().any(|error| matches!(
        error,
        crate::interpret::Error::Pass(error)
            if matches!(error.error().capability_error(),
                Some(qsc_rca::errors::Error::UseOfDynamicExponent(_)))
    )));
    assert!(original.iter().any(|error| matches!(
        error,
        crate::interpret::Error::Pass(error)
            if matches!(error.error().capability_error(),
                Some(qsc_rca::errors::Error::UseOfDynamicallySizedArray(_)))
    )));

    let errors = compile_source_to_qir_result(&source, Profile::Adaptive.into())
        .expect_err("RIF must still reject measurement-dependent allocation and exponents");
    assert_original_errors_with_fallback_note(&errors, &original);
}

#[test]
fn adaptive_fallback_mixed_errors_increment_preserves_simulation() {
    let source = POSITIVE
        .replace("@EntryPoint()", "")
        .replace("(n +", "(2 ^ n +");
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, &source);
    let result = interpreter.qirgen("Main()").expect("mixed-error fallback");
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert_eq!(
        eval_fragments(&mut interpreter, "Main()").to_string(),
        "(18, [Zero, Zero, Zero])"
    );
}

fn mixed_capability_errors() -> Vec<qsc_passes::Error> {
    use qsc_rca::errors::Error as RcaError;
    let span = PackageSpan {
        package: qsc_fir::fir::PackageId::CORE,
        span: Default::default(),
    };
    vec![
        qsc_passes::Error::CapabilitiesCk(RcaError::UseOfDynamicRange(span)),
        qsc_passes::Error::CapabilitiesCk(RcaError::UseOfDynamicArray(span)),
        qsc_passes::Error::CapabilitiesCk(RcaError::UseOfDynamicallySizedArray(span)),
        qsc_passes::Error::CapabilitiesCk(RcaError::UseOfDynamicExponent(span)),
    ]
}

#[test]
fn adaptive_fallback_accepts_mixed_capability_errors_for_full_adaptive() {
    assert!(adaptive_fallback_eligible(
        Profile::Adaptive.into(),
        &mixed_capability_errors(),
    ));
}

#[test]
fn adaptive_fallback_does_not_upgrade_base() {
    assert!(!adaptive_fallback_eligible(
        Profile::Base.into(),
        &mixed_capability_errors()
    ));
}

#[test]
fn adaptive_fallback_does_not_upgrade_adaptive_ri() {
    assert!(!adaptive_fallback_eligible(
        Profile::AdaptiveRI.into(),
        &mixed_capability_errors()
    ));
}

#[test]
fn adaptive_fallback_does_not_retry_explicit_rif() {
    assert!(!adaptive_fallback_eligible(
        Profile::AdaptiveRIF.into(),
        &mixed_capability_errors()
    ));
}

#[test]
fn adaptive_fallback_does_not_narrow_unrestricted() {
    assert!(!adaptive_fallback_eligible(
        TargetCapabilityFlags::all(),
        &mixed_capability_errors()
    ));
}

#[test]
fn adaptive_fallback_does_not_upgrade_a_custom_capability_mask() {
    assert!(!adaptive_fallback_eligible(
        TargetCapabilityFlags::Adaptive,
        &mixed_capability_errors()
    ));
}

#[test]
fn adaptive_fallback_genuine_dynamic_preserves_primary_errors() {
    let source = "@EntryPoint() operation Main() : Result[] { use q = Qubit(); let n = MResetZ(q) == One ? 3 | 2; use qs = Qubit[n]; MResetEachZ(qs) }";
    let original = strict_adaptive_errors(source);
    let errors = compile_source_to_qir_result(source, Profile::Adaptive.into())
        .expect_err("genuine dynamism");
    assert_original_errors_with_fallback_note(&errors, &original);
}

#[test]
fn adaptive_fallback_failed_retry_renders_every_source_diagnostic_without_panicking() {
    let errors = compile_source_to_qir_result(
        "@EntryPoint() operation Main() : Result[] { use q = Qubit(); let n = MResetZ(q) == One ? 3 | 2; use qs = Qubit[n]; MResetEachZ(qs) }",
        Profile::Adaptive.into(),
    ).expect_err("genuine dynamic allocation");
    let mut notes = 0;
    for error in errors {
        let mut rendered = String::new();
        miette::GraphicalReportHandler::new()
            .render_report(&mut rendered, &error)
            .expect("render diagnostic");
        assert!(rendered.contains("test.qs"), "{rendered}");
        if rendered.contains("Adaptive_RIF fallback was also rejected") {
            notes += 1;
        }
    }
    assert_eq!(notes, 1, "render exactly one failed-fallback note");
}

#[test]
fn failed_fallback_note_preserves_dependency_source_location() {
    let caps = Profile::Adaptive.into();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(caps);
    let source = "operation Allocate(n : Int) : Result[] { use qs = Qubit[n]; MResetEachZ(qs) }";
    let (unit, errors) = crate::compile::compile(
        &store,
        &[(std_id, None)],
        SourceMap::new([("dependency.qs".into(), source.into())], None),
        PackageType::Lib,
        caps,
        LanguageFeatures::default(),
    );
    assert!(errors.is_empty(), "{errors:?}");
    let dependency = store.insert(unit);
    let lo = u32::try_from(source.find("Qubit[n]").expect("allocation")).expect("offset");
    let diagnostic = qsc_passes::Error::AdaptiveFallbackProof(
        qsc_passes::AdaptiveFallbackAnalysisError::new(qsc_passes::Error::CapabilitiesCk(
            qsc_rca::errors::Error::UseOfDynamicallySizedArray(PackageSpan {
                package: qsc_lowerer::map_hir_package_to_fir(dependency),
                span: qsc_data_structures::span::Span { lo, hi: lo + 8 },
            }),
        )),
    );
    // The fallback source map is deliberately not the diagnostic's owner.
    let errors = crate::codegen::qir::map_pass_errors(&store, std_id, vec![diagnostic]);
    let mut rendered = String::new();
    miette::GraphicalReportHandler::new()
        .render_report(&mut rendered, &errors[0])
        .expect("render");
    assert!(rendered.contains("dependency.qs"), "{rendered}");
    assert!(rendered.contains("Qubit[n]"), "{rendered}");
}

#[test]
fn adaptive_fallback_rir_remains_strict() {
    let caps = Profile::Adaptive.into();
    let (std_id, store) = crate::compile::package_store_with_stdlib(caps);
    assert!(
        get_rir(
            source_map_from_source(POSITIVE),
            LanguageFeatures::default(),
            caps,
            store,
            &[(std_id, None)]
        )
        .is_err()
    );
}

#[test]
fn adaptive_fallback_rir_preview_recovers_with_coded_warning() {
    use miette::Diagnostic;
    let caps = Profile::Adaptive.into();
    let (std_id, store) = crate::compile::package_store_with_stdlib(caps);
    let result = crate::codegen::qir::get_rir_with_diagnostics(
        source_map_from_source(POSITIVE),
        LanguageFeatures::default(),
        caps,
        store,
        &[(std_id, None)],
    )
    .expect("RIR preview uses the effective QIR backend");
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert_eq!(result.rir.len(), 2);
    assert_eq!(
        result.rir,
        compile_source_to_rir(POSITIVE, Profile::AdaptiveRIF.into())
    );
    assert_ne!(
        result.rir[0], result.rir[1],
        "raw and transformed RIR must not be duplicated"
    );
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        result.warnings[0].code().expect("warning code").to_string(),
        "Qdk.Qsc.Codegen.AdaptiveProfileFallback",
    );
}

#[test]
fn adaptive_fallback_interpreter_rir_preview_preserves_strict_diagnostics() {
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, &POSITIVE.replace("@EntryPoint()", ""));
    let result = interpreter
        .get_rir_with_diagnostics("Main()")
        .expect("RIR preview");
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert!(interpreter.get_rir("Main()").is_err());
    assert_eq!(
        eval_fragments(&mut interpreter, "Main()").to_string(),
        "(13, [Zero, Zero, Zero])",
    );
}

#[test]
fn adaptive_fallback_rir_preview_rejects_genuine_dynamic_allocation() {
    let caps = Profile::Adaptive.into();
    let (std_id, store) = crate::compile::package_store_with_stdlib(caps);
    let errors = crate::codegen::qir::get_rir_with_diagnostics(
        source_map_from_source(
            "@EntryPoint() operation Main() : Result[] { use q = Qubit(); let n = MResetZ(q) == One ? 3 | 2; use qs = Qubit[n]; MResetEachZ(qs) }",
        ),
        LanguageFeatures::default(),
        caps,
        store,
        &[(std_id, None)],
    )
    .err()
    .expect("RIF cannot prove measurement-dependent allocation static");
    assert!(matches!(
        errors.first(),
        Some(crate::interpret::Error::Pass(_))
    ));
}

fn detailed(source: &str) -> crate::codegen::qir::QirGenerationResult {
    let caps = Profile::Adaptive.into();
    let (std_id, store) = crate::compile::package_store_with_stdlib(caps);
    crate::codegen::qir::get_qir(
        source_map_from_source(source),
        LanguageFeatures::default(),
        caps,
        store,
        &[(std_id, None)],
    )
    .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors)))
}

#[test]
fn adaptive_fallback_source_api_returns_source_located_warning() {
    use miette::{Diagnostic, Severity};
    let result = detailed(POSITIVE);
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert_eq!(result.warnings.len(), 1);
    let warning = &result.warnings[0];
    assert_eq!(
        warning.code().expect("warning code").to_string(),
        "Qdk.Qsc.Codegen.AdaptiveProfileFallback"
    );
    assert_eq!(warning.severity(), Some(Severity::Warning));
    assert!(warning.source_code().is_some());
    assert!(
        warning
            .labels()
            .expect("warning source span")
            .next()
            .is_some()
    );
    assert!(warning.to_string().contains("QIR 1.0"));
}

#[test]
fn adaptive_fallback_normal_success_stays_adaptive() {
    let source = "@EntryPoint() operation Main() : Result { use q = Qubit(); MResetZ(q) }";
    let result = detailed(source);
    assert_eq!(result.capabilities, Profile::Adaptive.into());
    assert!(result.warnings.is_empty());
    assert_eq!(
        result.qir,
        compile_source_to_qir(source, Profile::Adaptive.into())
    );
}

#[test]
fn adaptive_fallback_user_failure_has_no_retry() {
    let errors = compile_source_to_qir_result(
        "@EntryPoint() operation Main() : Int { fail \"user failure\"; }",
        Profile::Adaptive.into(),
    )
    .expect_err("user fail");
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        errors[0],
        crate::interpret::Error::PartialEvaluation(_)
    ));
}

fn interpreter_with_fallback_entry() -> crate::interpret::Interpreter {
    let caps = Profile::Adaptive.into();
    let (std_id, store) = crate::compile::package_store_with_stdlib(caps);
    crate::interpret::Interpreter::new(
        source_map_from_source(POSITIVE),
        PackageType::Exe,
        caps,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors)))
}

#[test]
fn adaptive_fallback_constructor_admits_a_supported_entry() {
    let mut interpreter = interpreter_with_fallback_entry();
    assert_eq!(
        interpreter
            .qirgen("test.Main()")
            .expect("QIR")
            .warnings
            .len(),
        1,
    );
}

#[test]
fn adaptive_fallback_repeated_generation_returns_identical_qir_and_fresh_warnings() {
    let mut interpreter = interpreter_with_fallback_entry();
    let first = interpreter
        .qirgen_with_diagnostics("test.Main()")
        .expect("expression QIR");
    let second = interpreter
        .qirgen_with_diagnostics("test.Main()")
        .expect("repeated QIR");
    assert_eq!(first.qir, second.qir);
    assert_eq!(first.warnings.len(), 1);
    assert_eq!(second.warnings.len(), 1);
}

#[test]
fn adaptive_fallback_callable_matches_expression_qir() {
    let mut interpreter = interpreter_with_fallback_entry();
    let first = interpreter.qirgen("test.Main()").expect("expression QIR");
    let callable = eval_fragments(&mut interpreter, "test.Main");
    let result = interpreter
        .qirgen_from_callable(&callable, Value::unit())
        .expect("callable QIR");
    assert_eq!(result.qir, first.qir);
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert!(matches!(
        first.warnings.as_slice(),
        [crate::interpret::Error::AdaptiveProfileFallback(_)]
    ));
    assert!(matches!(
        result.warnings.as_slice(),
        [crate::interpret::Error::AdaptiveProfileFallback(_)]
    ));
}

#[test]
fn adaptive_fallback_generation_preserves_subsequent_simulation() {
    let mut interpreter = interpreter_with_fallback_entry();
    interpreter
        .qirgen_with_diagnostics("test.Main()")
        .expect("QIR");
    assert_eq!(
        eval_fragments(&mut interpreter, "test.Main()").to_string(),
        "(13, [Zero, Zero, Zero])"
    );
}

#[test]
fn adaptive_fallback_does_not_change_interpreters_strict_rir_profile() {
    let mut interpreter = interpreter_with_fallback_entry();
    interpreter
        .qirgen_with_diagnostics("test.Main()")
        .expect("QIR");
    assert!(interpreter.get_rir("test.Main()").is_err());
}

#[test]
fn adaptive_fallback_failed_request_does_not_contaminate_later_success() {
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(
        &mut interpreter,
        &POSITIVE
            .replace(
                "(n + (MResetZ(q) == One ? 10 | 0), rs)",
                "let _ = (n + (MResetZ(q) == One ? 10 | 0), rs); fail \"expected failure\";",
            )
            .replace("@EntryPoint()", ""),
    );
    assert!(interpreter.qirgen_with_diagnostics("Main()").is_err());
    let result = interpreter
        .qirgen_with_diagnostics("{ use q = Qubit(); MResetZ(q) }")
        .expect("unrelated supported entry");
    assert!(result.warnings.is_empty());
    assert_eq!(result.capabilities, Profile::Adaptive.into());
}

#[test]
fn adaptive_fallback_retains_supported_measurement_driven_loops_in_adaptive() {
    let result = detailed(
        "@EntryPoint() operation Main() : Result { use q = Qubit(); mutable stop = false; while not stop { X(q); set stop = MResetZ(q) == One; } MResetZ(q) }",
    );
    assert!(result.warnings.is_empty());
    assert_eq!(result.capabilities, Profile::Adaptive.into());
    assert!(result.qir.contains("br i1"));
}

#[test]
fn adaptive_fallback_incremental_admission_and_rollback() {
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, &POSITIVE.replace("@EntryPoint()", ""));
    let before = interpreter
        .qirgen_with_diagnostics("Main()")
        .expect("increment QIR");
    let mut cursor = Cursor::new(Vec::new());
    let mut receiver = CursorReceiver::new(&mut cursor);
    assert!(interpreter.eval_fragments(&mut receiver,
        "operation Bad() : Result[] { use q = Qubit(); let n = MResetZ(q) == One ? 3 | 2; use qs = Qubit[n]; MResetEachZ(qs) }").is_err());
    let after = interpreter
        .qirgen_with_diagnostics("Main()")
        .expect("rollback QIR");
    assert_eq!(before.qir, after.qir);
    assert_eq!(
        eval_fragments(&mut interpreter, "Main()").to_string(),
        "(13, [Zero, Zero, Zero])"
    );
}

#[test]
fn adaptive_fallback_preserves_the_original_config_selected_definition() {
    let source = POSITIVE.replace("operation Count(", "@Config(CallSupport) operation Count(");
    let source =
        format!("{source}\n@Config(not CallSupport) operation Count(q : Qubit) : Int {{ 99 }}");
    let result = detailed(&source);
    assert_eq!(result.qir, detailed(POSITIVE).qir);
}

#[test]
fn adaptive_fallback_one_logical_retry_only() {
    let span = PackageSpan {
        package: qsc_fir::fir::PackageId::CORE,
        span: Default::default(),
    };
    let mut attempts = Vec::new();
    let result = crate::codegen::qir::run_rca_with_adaptive_fallback(
        Profile::Adaptive.into(),
        |capabilities| {
            attempts.push(capabilities);
            if capabilities == Profile::Adaptive.into() {
                Err(vec![qsc_passes::Error::CapabilitiesCk(
                    qsc_rca::errors::Error::UseOfDynamicArray(span),
                )])
            } else {
                Ok(Default::default())
            }
        },
    )
    .expect("narrower proof");
    assert_eq!(
        attempts,
        vec![Profile::Adaptive.into(), Profile::AdaptiveRIF.into()]
    );
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
}

#[test]
fn adaptive_fallback_mixed_errors_retry_once_and_retain_first_reason() {
    let span = PackageSpan {
        package: qsc_fir::fir::PackageId::CORE,
        span: Default::default(),
    };
    let mut attempts = Vec::new();
    let result = crate::codegen::qir::run_rca_with_adaptive_fallback(
        Profile::Adaptive.into(),
        |capabilities| {
            attempts.push(capabilities);
            if capabilities == Profile::Adaptive.into() {
                Err(vec![
                    qsc_passes::Error::CapabilitiesCk(qsc_rca::errors::Error::UseOfDynamicArray(
                        span,
                    )),
                    qsc_passes::Error::CapabilitiesCk(
                        qsc_rca::errors::Error::UseOfDynamicExponent(span),
                    ),
                ])
            } else {
                Ok(Default::default())
            }
        },
    )
    .expect("mixed errors permit a narrower analysis");
    assert_eq!(
        attempts,
        vec![Profile::Adaptive.into(), Profile::AdaptiveRIF.into()]
    );
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert!(matches!(result.fallback_reason,
        Some(qsc_passes::Error::CapabilitiesCk(qsc_rca::errors::Error::UseOfDynamicArray(reason)))
            if reason == span
    ));
}

#[test]
fn adaptive_fallback_successful_adaptive_analysis_does_not_retry() {
    let mut attempts = Vec::new();
    let result = crate::codegen::qir::run_rca_with_adaptive_fallback(
        Profile::Adaptive.into(),
        |capabilities| {
            attempts.push(capabilities);
            Ok(Default::default())
        },
    )
    .expect("Adaptive analysis succeeds");
    assert_eq!(attempts, vec![Profile::Adaptive.into()]);
    assert_eq!(result.capabilities, Profile::Adaptive.into());
    assert!(result.fallback_reason.is_none());
}

#[test]
fn adaptive_fallback_failed_retry_preserves_original_errors_with_one_note() {
    let span = PackageSpan {
        package: qsc_fir::fir::PackageId::CORE,
        span: Default::default(),
    };
    let original = vec![
        qsc_passes::Error::CapabilitiesCk(qsc_rca::errors::Error::UseOfDynamicRange(span)),
        qsc_passes::Error::CapabilitiesCk(qsc_rca::errors::Error::UseOfDynamicExponent(span)),
    ];
    let mut count = 0;
    let errors =
        crate::codegen::qir::run_rca_with_adaptive_fallback(Profile::Adaptive.into(), |_| {
            count += 1;
            if count == 1 {
                Err(original.clone())
            } else {
                Err(vec![qsc_passes::Error::CapabilitiesCk(
                    qsc_rca::errors::Error::LoopWithDynamicCondition(span),
                )])
            }
        })
        .err()
        .expect("failed retry");
    assert_eq!(count, 2);
    assert_original_errors_with_fallback_note(&errors, &original);
}

#[test]
fn adaptive_fallback_library_constructor_admits_closed_main() {
    let caps = Profile::Adaptive.into();
    let (std_id, store) = crate::compile::package_store_with_stdlib(caps);
    let mut interpreter = crate::interpret::Interpreter::new(
        source_map_from_source(&POSITIVE.replace("@EntryPoint()", "")),
        PackageType::Lib,
        caps,
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
        Default::default(),
    )
    .unwrap_or_else(|errors| panic!("{}", format_interpret_errors(errors)));
    assert_eq!(
        interpreter
            .qirgen_with_diagnostics("test.Main()")
            .expect("library QIR")
            .warnings
            .len(),
        1
    );
}

#[test]
fn adaptive_fallback_synthetic_callable_root_preserves_arguments() {
    let mut interpreter = interpreter_with_capabilities(Profile::Adaptive.into());
    eval_fragments(&mut interpreter, &POSITIVE.replace("@EntryPoint()", ""));
    eval_fragments(
        &mut interpreter,
        "operation Use(read : Unit => (Int, Result[])) : (Int, Result[]) { read() }",
    );
    let target = eval_fragments(&mut interpreter, "Use");
    let argument = eval_fragments(&mut interpreter, "Main");
    let result = interpreter
        .qirgen_from_callable_with_diagnostics(&target, argument)
        .expect("synthetic QIR");
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(
        result.qir,
        interpreter
            .qirgen_with_diagnostics("Use(Main)")
            .expect("expression QIR")
            .qir
    );
    assert_eq!(
        eval_fragments(&mut interpreter, "Use(Main)").to_string(),
        "(13, [Zero, Zero, Zero])"
    );
}

fn specialization_warning_calls() -> String {
    use std::fmt::Write;
    (1..=11).fold(String::new(), |mut calls, angle| {
        writeln!(calls, "Apply(q1 => Rx({angle}.0, q1), q);").expect("write source");
        calls
    })
}

#[test]
fn adaptive_fallback_normal_generation_preserves_transform_warning_message_and_help() {
    let calls = specialization_warning_calls();
    let source = format!(
        "operation Apply(op : Qubit => Unit, q : Qubit) : Unit {{ op(q); }}
         @EntryPoint() operation Main() : Unit {{ use q = Qubit(); {calls} }}"
    );
    let result = detailed(&source);
    assert!(!result.warnings.is_empty());
    assert!(
        result
            .warnings
            .iter()
            .all(|warning| matches!(warning, crate::interpret::Error::FirTransform(_)))
    );
    assert!(result.qir.contains("__quantum__qis__rx__body"));
    let message = crate::error::diagnostic_message(&result.warnings[0]);
    assert!(
        message.starts_with("higher-order function `Apply"),
        "{message}"
    );
    assert!(
        message.contains("generated 11 specializations, exceeding the warning threshold"),
        "{message}"
    );
    assert!(
        message.contains("help: consider reducing the number of distinct callable arguments"),
        "{message}"
    );
    assert!(!message.contains("FIR transform error"), "{message}");
}

#[test]
fn adaptive_fallback_retains_transform_warnings_alongside_the_fallback_warning() {
    let source = format!(
        "operation Apply(op : Qubit => Unit, q : Qubit) : Unit {{ op(q); }}\n{}",
        POSITIVE.replace(
            "let n = Count(q);",
            &format!("{} let n = Count(q);", specialization_warning_calls())
        ),
    );
    let result = detailed(&source);
    assert_eq!(result.capabilities, Profile::AdaptiveRIF.into());
    assert_eq!(
        result
            .warnings
            .iter()
            .filter(|warning| matches!(
                warning,
                crate::interpret::Error::AdaptiveProfileFallback(_)
            ))
            .count(),
        1
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| matches!(warning, crate::interpret::Error::FirTransform(_)))
    );
}

#[test]
fn adaptive_fallback_failed_generation_has_no_success_warning() {
    let source = POSITIVE.replace(
        "(n + (MResetZ(q) == One ? 10 | 0), rs)",
        "let _ = (n + (MResetZ(q) == One ? 10 | 0), rs); fail \"failed after narrower proof\";",
    );
    let errors =
        compile_source_to_qir_result(&source, Profile::Adaptive.into()).expect_err("PE must fail");
    assert_eq!(errors.len(), 1);
    assert!(matches!(
        errors[0],
        crate::interpret::Error::PartialEvaluation(_)
    ));
}

#[test]
fn frontend_type_error_returns_only_compile_diagnostic() {
    let errors = compile_source_to_qir_result(
        "@EntryPoint() operation Main() : Int { true }",
        Profile::Adaptive.into(),
    )
    .expect_err("type errors must fail before RCA");
    assert!(matches!(
        errors.as_slice(),
        [crate::interpret::Error::Compile(_)]
    ));
}

#[test]
fn adaptive_fallback_empty_failure_does_not_reanalyze() {
    let mut count = 0;
    let result =
        crate::codegen::qir::run_rca_with_adaptive_fallback(Profile::Adaptive.into(), |_| {
            count += 1;
            Err(Vec::new())
        });
    assert!(result.is_err());
    assert_eq!(count, 1);
}
