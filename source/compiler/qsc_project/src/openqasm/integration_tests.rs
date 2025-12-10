// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use qsc_openqasm_parser::semantic::QasmSemanticParseResult;

use crate::{FileSystem, ProjectType, StdFs};
use miette::Report;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn get_test_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("tests")
        .join("openqasm_projects")
}

fn parse_file(file_name: &'static str) -> (Arc<str>, QasmSemanticParseResult) {
    let test_dir = get_test_dir();
    let test_file = test_dir.join(file_name);
    parse_file_with_contents(&test_file, None)
}

fn parse_file_with_contents<P: AsRef<Path>>(
    test_file: P,
    source: Option<Arc<str>>,
) -> (Arc<str>, QasmSemanticParseResult) {
    let fs = StdFs;
    let project = fs.load_openqasm_project(test_file.as_ref(), source);
    let ProjectType::OpenQASM(sources) = project.project_type else {
        panic!("Expected OpenQASM project type");
    };
    let result = qsc_openqasm_parser::semantic::parse_sources(&sources);
    (
        test_file.as_ref().display().to_string().as_str().into(),
        result,
    )
}

#[test]
fn test_real_simple_qasm_file() {
    let file_name = "simple.qasm";
    let (test_file, result) = parse_file(file_name);

    // Should succeed - QasmSource always returns, check for errors
    let errors = result.errors();
    assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    assert!(!result.has_errors(), "Should not have any errors");

    // Verify that simple.qasm was loaded
    let source = result.source;
    assert_eq!(source.path().as_ref(), test_file.as_ref());
}

#[test]
fn test_real_file_with_includes() {
    let file_name = "with_includes.qasm";
    let (_, result) = parse_file(file_name);

    let errors = result.errors();
    assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    assert!(!result.has_errors(), "Should not have any errors");

    let includes = result.source.includes();
    // Note: Only one include should be present since stdgates.inc is ignored
    assert_eq!(
        includes.len(),
        1,
        "Should have one include (stdgates.inc is ignored)"
    );

    // Check that the included file is correct
    let included_file = &includes[0];
    let test_dir = get_test_dir();
    let expected_include_path = test_dir.join("included.qasm");
    assert_eq!(
        included_file.path().as_ref(),
        expected_include_path.to_string_lossy()
    );

    // verify that the included file content is present
    result
        .symbols
        .get_symbol_by_name("my_gate")
        .expect("Should find my_gate in symbols");

    // verify some stdgates.inc symbols are included
    for gate in &["h", "x", "y", "z"] {
        assert!(
            result.symbols.get_symbol_by_name(gate).is_ok(),
            "Should find gate {gate} in symbols"
        );
    }
}

#[test]
fn test_real_missing_include() {
    let file_name = "missing_include.qasm";
    let (_, result) = parse_file(file_name);

    assert!(result.has_errors(), "Should indicate presence of errors");

    let all_errors = result.all_errors();
    assert!(
        !all_errors.is_empty(),
        "Should have errors for missing file"
    );

    let error_strings: Vec<_> = all_errors.iter().map(|e| format!("{e:?}\n")).collect();
    assert!(
        error_strings
            .iter()
            .any(|e| e.contains("This file includes a missing file")),
        "Should have file system error, got: {all_errors:?}"
    );
}

#[test]
fn test_real_circular_includes() {
    let file_name = "circular_a.qasm";
    let (_, result) = parse_file(file_name);

    assert!(result.has_errors(), "Should indicate presence of errors");

    let all_errors = result.all_errors();
    assert!(
        !all_errors.is_empty(),
        "Should have errors for circular dependency"
    );

    let error_strings: Vec<_> = all_errors.iter().map(|e| format!("{e:?}")).collect();
    assert!(
        error_strings.iter().any(|e| e.contains("CyclicInclude")),
        "Should have circular dependency error, got: {all_errors:?}"
    );
}

#[test]
fn test_real_duplicate_includes() {
    let file_name = "duplicate_includes.qasm";
    let (_, result) = parse_file(file_name);

    assert!(result.has_errors(), "Should indicate presence of errors");

    let all_errors = result.all_errors();
    assert!(
        !all_errors.is_empty(),
        "Should have errors for duplicate includes"
    );

    let error_strings: Vec<_> = all_errors.iter().map(|e| format!("{e:?}")).collect();
    assert!(
        error_strings.iter().any(|e| e.contains("MultipleInclude")),
        "Should have duplicate include error, got: {all_errors:?}"
    );
}

#[test]
fn test_relative_path_file_includes() {
    let file_name = "relative_files.qasm";
    let (_, result) = parse_file(file_name);

    if result.has_errors() {
        let all_errors = result.all_errors();
        assert!(
            all_errors.is_empty(),
            "Should not have errors for relative path includes, got: {all_errors:?}"
        );
    }

    // verify that the includes were loaded correctly
    for gate in &["gate_a", "gate_b"] {
        assert!(
            result.symbols.get_symbol_by_name(gate).is_ok(),
            "Should find gate {gate} in symbols"
        );
    }
}

#[test]
fn unsaved_files_can_ref_stdgates() {
    let file_name = "untitled:Untitled-1";
    let contents = r#"
    OPENQASM 3.0;
    include "stdgates.inc";

    // This file includes a missing file
    qreg q[1];
    h q[0];
    "#;
    let (_, result) = parse_file_with_contents(file_name, Some(contents.into()));

    if result.has_errors() {
        let all_errors = result.all_errors();
        assert!(
            all_errors.is_empty(),
            "Should not have errors for built-in includes, got: {all_errors:?}"
        );
    }
}

#[test]
fn unsaved_files_cannot_ref_relative_includes() {
    let file_name = "untitled:Untitled-1";
    let contents = r#"
    OPENQASM 3.0;
    include "stdgates.inc";
    include "nonexistent.qasm";

    // This file includes a missing file
    qreg q[1];
    h q[0];
    "#;
    let (_, result) = parse_file_with_contents(file_name, Some(contents.into()));

    assert!(result.has_errors(), "Should indicate presence of errors");

    let all_errors = result.errors();

    expect![[r#"
        [  x Not Found: Could not resolve include file: nonexistent.qasm
           ,-[untitled:Untitled-1:4:5]
         3 |     include "stdgates.inc";
         4 |     include "nonexistent.qasm";
           :     ^^^^^^^^^^^^^^^^^^^^^^^^^^^
         5 | 
           `----
        ]"#]]
    .assert_eq(&format!(
        "{:?}",
        all_errors
            .iter()
            .map(|e| Report::new(e.clone()))
            .collect::<Vec<_>>()
    ));
}
