// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::env::current_dir;
use std::fs::read_dir;
use std::fs::read_to_string;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use crate::{compile_kata, verify_kata};
use qsc_eval::output::GenericReceiver;

fn katas_qsharp_source_dir() -> PathBuf {
    current_dir()
        .expect("Unable to get the katas crate current directory")
        .join("qs")
}

fn validate_exercise(exercise_dir: &Path) {
    let exercise_name = format!(
        "{}",
        exercise_dir
            .file_name()
            .expect("Unable to obtain exercice name.")
            .to_string_lossy()
    );
    let mut verification_source_file = PathBuf::from(exercise_dir);
    verification_source_file.push("verify.qs");
    let verification_source =
        read_to_string(verification_source_file).expect("Unable to read verification file.");

    // Validate that both the placeholder and the reference implementation compile successfully.
    let mut reference_file_path = PathBuf::from(exercise_dir);
    reference_file_path.push("reference.qs");
    let reference_source = read_to_string(reference_file_path)
        .expect("Unable to read reference source implementation file.");
    let mut placeholder_file_path = PathBuf::from(exercise_dir);
    placeholder_file_path.push("placeholder.qs");
    let placeholder_source = read_to_string(placeholder_file_path)
        .expect("Unable to read placeholder source implementation file.");
    let sources = vec![reference_source.clone(), placeholder_source.clone()];
    for source in &sources {
        let kata_compilation = compile_kata(verification_source.as_str(), source.as_str());
        let kata_errors = match kata_compilation {
            Ok((_, _)) => None,
            Err(e) => Some(e),
        };

        assert!(
            kata_errors.is_none(),
            "Kata does not compile for exercise '{exercise_name}'. {kata_errors:?}"
        );
    }

    // Validate that the reference implementation yields success and the placeholder implementation yields failure.
    let mut stdout = io::stdout();
    let mut out = GenericReceiver::new(&mut stdout);
    let reference_succeeds = verify_kata(
        verification_source.as_str(),
        reference_source.as_str(),
        &mut out,
    );
    assert!(
        reference_succeeds,
        "Reference implementation for exercise '{exercise_name}' expected to succeed but failed."
    );
    let _placeholder_fails = !verify_kata(
        verification_source.as_str(),
        placeholder_source.as_str(),
        &mut out,
    );
    // N.B. Since verify_kata is doing evaluation, but it is not possible to determine correctness of some katas until
    //      the controlled functor is supported.
    //assert!(
    //    _placeholder_fails,
    //    "Placeholder implementation for exercise '{exercise_name}' expected to fail but succeeded.",
    //);
}

fn validate_module(module_dir: &PathBuf) {
    for entry in read_dir(module_dir).expect("Unable to read module dir") {
        let path = entry
            .expect("No path for entry in module directory.")
            .path();
        if path.is_dir() {
            validate_exercise(&path);
        }
    }
}

#[test]
fn verify_single_qubit_gates_module() {
    let mut module_dir = katas_qsharp_source_dir();
    module_dir.push("single_qubit_gates");
    validate_module(&module_dir);
}
