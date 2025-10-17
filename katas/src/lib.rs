// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qsc::{
    PackageType, SourceContents, SourceMap, SourceName,
    interpret::{Error, Interpreter, Value, output::Receiver},
    target::Profile,
};

use qsc::LanguageFeatures;

pub const EXAMPLE_ENTRY: &str = "Kata.RunExample()";

pub const EXERCISE_ENTRY: &str = "Kata.Verification.CheckSolution()";

/// # Errors
///
/// Returns a vector of errors if compilation or evaluation failed.
///
/// # Panics
///
/// Will panic if evaluation does not return a boolean as result.
pub fn check_solution(
    exercise_sources: Vec<(SourceName, SourceContents)>,
    receiver: &mut impl Receiver,
) -> Result<bool, Vec<Error>> {
    let source_map = SourceMap::new(exercise_sources, Some(EXERCISE_ENTRY.into()));
    let (std_id, store) = qsc::compile::package_store_with_stdlib(Profile::Unrestricted.into());

    let mut interpreter: Interpreter = Interpreter::new(
        source_map,
        PackageType::Exe,
        Profile::Unrestricted.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )?;

    interpreter.eval_entry(receiver).map(|value| {
        if let Value::Bool(success) = value {
            success
        } else {
            panic!("exercise verification did not return a boolean")
        }
    })
}
