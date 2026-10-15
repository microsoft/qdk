// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod backends;

use super::QirBackend;
use qsc::{LanguageFeatures, SourceMap, codegen::qir::get_qir, target::Profile};
use std::path::PathBuf;

struct QirFile(PathBuf);

impl QirFile {
    /// Creates a unique path for a temporary QIR file.
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "qep-qsharp-{}-{}.ll",
            std::process::id(),
            rand::random::<u64>()
        )))
    }
}

impl Drop for QirFile {
    /// Removes the temporary QIR file if it was created.
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!(
                "Failed to clean up QEP QIR file {}: {error}",
                self.0.display()
            );
        }
    }
}

/// Compiles Q# source to a temporary QIR file.
fn compile_qsharp_to_qir(source: &str, profile: Profile) -> QirFile {
    let capabilities = profile.into();
    let (std_id, store) = qsc::compile::package_store_with_stdlib(capabilities);
    let qir = get_qir(
        SourceMap::new([("test.qs".into(), source.into())], None),
        LanguageFeatures::default(),
        capabilities,
        store,
        &[(std_id, None)],
    )
    .unwrap_or_else(|errors| {
        panic!(
            "{}",
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let file = QirFile::new();
    std::fs::write(&file.0, qir).expect("failed to write compiled QIR");
    file
}
 