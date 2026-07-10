// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This test validates that all `#[diagnostic(code("..."))]` attributes in the
//! source tree follow the naming convention: the first segment must be a known
//! prefix (`Qsc`, `Qasm`, `Stim`, or `Qre`), followed by one or more
//! PascalCase segments containing only ASCII alphanumeric characters.
//!
//! For example: `Qsc.Resolve.NotFound`, `Qasm.Lowerer.CannotCast`.

use std::fs;
use std::path::{Path, PathBuf};

/// Recursively collect all `.rs` files under `dir`.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Check that a diagnostic code string matches the convention:
/// two or more dot-separated segments where the first segment is a known
/// prefix (`Qsc`, `Qasm`, `Stim`, or `Qre`) and subsequent segments each
/// start with an uppercase ASCII letter and contain only ASCII alphanumeric
/// characters.
fn is_valid_diagnostic_code(code: &str) -> bool {
    let parts: Vec<&str> = code.split('.').collect();
    parts.len() >= 3
        && matches!(parts[0], "Qdk")
        && matches!(parts[1], "Qsc" | "Qasm" | "Stim" | "Qre")
        && parts[2..].iter().all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|c| c.is_ascii_uppercase())
                && chars.all(|c| c.is_ascii_alphanumeric())
        })
}

/// Extract all diagnostic code strings from source text.
/// Looks for lines that are attributes like: `#[diagnostic(code("SomeCode"))]`
fn extract_diagnostic_codes(source: &str, path: &Path) -> Vec<(usize, String)> {
    // Don't do anything clever about wrapped lines since codes are short
    let prefix = "#[diagnostic(code(\"";
    let mut results = Vec::new();
    for (line_number, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        // Only match actual attributes (lines starting with #[), not comments or strings
        if !trimmed.starts_with("#[") {
            continue;
        }
        if let Some(start) = trimmed.find(prefix) {
            let after = &trimmed[start + prefix.len()..];
            let end = after.find("\"))]").unwrap_or_else(|| {
                panic!(
                    "{}:{}: found opening #[diagnostic(code(\"...but no closing \"))] delimiter",
                    path.display(),
                    line_number + 1
                )
            });
            let code = &after[..end];
            results.push((line_number + 1, code.to_string()));
        }
    }
    results
}

#[test]
fn all_diagnostic_codes_follow_naming_convention() {
    // Navigate from this crate's manifest dir (source/language_service) up to source/
    let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

    let mut rs_files = Vec::new();
    collect_rs_files(&source_dir, &mut rs_files);

    let mut failures = Vec::new();

    for path in &rs_files {
        let contents = fs::read_to_string(path).expect("should be able to read source file");
        for (line, code) in extract_diagnostic_codes(&contents, path) {
            if !is_valid_diagnostic_code(&code) {
                let relative = path.strip_prefix(&source_dir).unwrap_or(path).display();
                failures.push(format!("  {relative}:{line}: \"{code}\""));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "The following diagnostic codes do not follow the naming convention \
         (expected dot-separated PascalCase segments, e.g. \"Qsc.Resolve.NotFound\"):\n{}",
        failures.join("\n")
    );
}

#[test]
fn diagnostic_code_validation_helper_works() {
    // Valid codes
    assert!(is_valid_diagnostic_code("Qdk.Qsc.Resolve.NotFound"));
    assert!(is_valid_diagnostic_code("Qdk.Qasm.Lowerer.CannotCast"));
    assert!(is_valid_diagnostic_code("Qdk.Stim.UnrecognizedCharacter"));
    assert!(is_valid_diagnostic_code("Qdk.Qre.MaximumErrorExceeded"));
    assert!(is_valid_diagnostic_code(
        "Qdk.Qsc.Estimates.IOError.CannotOpenFile"
    ));

    // Invalid codes
    assert!(!is_valid_diagnostic_code("NotDotted"));
    assert!(!is_valid_diagnostic_code("qsc.resolve.notFound"));
    assert!(!is_valid_diagnostic_code("Qsc.resolve.NotFound"));
    assert!(!is_valid_diagnostic_code("Qsc..NotFound"));
    assert!(!is_valid_diagnostic_code("Qsc.Not Found"));
    assert!(!is_valid_diagnostic_code("Qsc.Not_Found"));
    assert!(!is_valid_diagnostic_code(".Qsc.Foo"));
    assert!(!is_valid_diagnostic_code("Qsc.Foo."));
    assert!(!is_valid_diagnostic_code("Foo.Bar.Baz"));
    assert!(!is_valid_diagnostic_code("Unknown.Something"));
}
