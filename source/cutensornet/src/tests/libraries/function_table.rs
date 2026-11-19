// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rule every function table's rows follow: each field is named for
//! the function it holds. Each library's tests check its tables against it.

/// Checks that each of `rows`, `(field, symbol, required)`, names its field
/// for its symbol: the field's words appear, in order, among the words of
/// the symbol once `prefix` is removed (`get_cudart_version` for
/// `cutensornetGetCudartVersion`, `create_svd_config` for
/// `cutensornetCreateTensorSVDConfig`).
///
/// The compiler checks each row's type against its symbol's declaration,
/// but two functions of one signature, such as cuTensorNet's two version
/// functions, would compile swapped; the rule catches the swap.
pub(super) fn assert_each_field_is_named_for_its_symbol(rows: &[(&str, &str, bool)], prefix: &str) {
    for &(field, symbol, _) in rows {
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
