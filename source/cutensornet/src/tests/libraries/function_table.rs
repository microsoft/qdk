// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rule every function table's rows follow: each field is named for
//! the function it holds, and holds a function no other field holds. Each
//! library's tests check its tables against it; the tests here check what
//! every table does when it is filled.

use crate::tests::stand_ins::StandInResolver;
use crate::{
    LibraryFault,
    libraries::{
        cuda_runtime::CudaFunctions, cutensornet::CuTensorNetFunctions,
        function_table::FunctionTable,
    },
};

/// Checks that each of `rows`, `(field, symbol, required)`, names its field
/// for its symbol: the field's words appear, in order, among the words of
/// the symbol once `prefix` is removed (`get_cudart_version` for
/// `cutensornetGetCudartVersion`, `create_svd_config` for
/// `cutensornetCreateTensorSVDConfig`); and that no symbol is held by two
/// rows, which the first check lets through when one field's words are
/// among both symbols' (`get_version` holding `cutensornetGetCudartVersion`
/// beside `get_cudart_version`).
///
/// The compiler checks each row's type against its symbol's declaration,
/// but two functions of one signature, such as cuTensorNet's two version
/// functions, would compile swapped or repeated; the rule catches both.
pub(super) fn assert_each_field_is_named_for_its_symbol(rows: &[(&str, &str, bool)], prefix: &str) {
    for (i, &(field, symbol, _)) in rows.iter().enumerate() {
        assert!(
            rows[..i].iter().all(|row| row.1 != symbol),
            "{symbol} is held by two fields, one of them {field}"
        );
        let name = symbol
            .strip_prefix(prefix)
            .unwrap_or_else(|| panic!("{symbol} starts with {prefix}"));
        let words = words_of(name);
        let mut remaining = words.iter();
        assert!(
            field
                .split('_')
                .all(|word| remaining.any(|candidate| candidate == word)),
            "the field {field} is named for {symbol} ({words:?})"
        );
    }
}

/// A library that lacks required functions is refused, naming each one in
/// its table's order, so that the user fixes them all at once.
#[test]
fn every_missing_required_function_is_reported_in_table_order() {
    for (table, rows, resolve) in [
        (
            "CudaFunctions",
            CudaFunctions::ROWS,
            faults_of::<CudaFunctions> as fn(&StandInResolver) -> Option<Vec<LibraryFault>>,
        ),
        (
            "CuTensorNetFunctions",
            CuTensorNetFunctions::ROWS,
            faults_of::<CuTensorNetFunctions>,
        ),
    ] {
        let required: Vec<_> = rows.iter().filter(|row| row.2).map(|row| row.1).collect();
        let resolver = required
            .iter()
            .fold(StandInResolver::audited(), |resolver, &symbol| {
                resolver.without(symbol)
            });
        let expected: Vec<_> = required
            .into_iter()
            .map(|symbol| LibraryFault::MissingRequiredSymbol { symbol })
            .collect();
        assert_eq!(resolve(&resolver), Some(expected), "{table}");
    }
}

/// The faults of filling the table `T` through `resolver`, if it fails.
fn faults_of<T: FunctionTable>(resolver: &StandInResolver) -> Option<Vec<LibraryFault>> {
    T::resolve(resolver).err()
}

/// The lower-case words of a camel-case name, an acronym being one word:
/// `TensorSVDConfig` is `tensor`, `svd`, `config`.
fn words_of(name: &str) -> Vec<String> {
    let characters: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    for (i, &character) in characters.iter().enumerate() {
        let starts_word = i == 0
            || character.is_uppercase()
                && (characters[i - 1].is_lowercase()
                    || characters
                        .get(i + 1)
                        .is_some_and(|next| next.is_lowercase()));
        if starts_word {
            words.push(String::new());
        }
        words
            .last_mut()
            .expect("the first character starts a word")
            .extend(character.to_lowercase());
    }
    words
}
