// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

pub mod collector;
pub mod word_kinds;

use collector::ValidWordCollector;
pub use word_kinds::{CompletionContext, WordKinds};

use super::{ParserContext, prgm};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompletionDirective {
    Annotation(String),
    Pragma(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    pub words: WordKinds,
    pub context: Option<CompletionContext>,
    pub directive: Option<CompletionDirective>,
}

/// Returns the parser completion information at a source offset.
#[must_use]
pub fn completion_at_offset_in_source(input: &str, at_offset: u32) -> Completion {
    let mut collector = ValidWordCollector::new(at_offset);
    let mut scanner = ParserContext::with_word_collector(input, &mut collector);
    let _ = prgm::parse(&mut scanner);
    collector.into_completion()
}

/// Returns the words that would be valid syntax at a particular offset
/// in the given source file (using the source file parser).
///
/// This is useful for providing completions in an editor.
#[must_use]
pub fn possible_words_at_offset_in_source(input: &str, at_offset: u32) -> WordKinds {
    completion_at_offset_in_source(input, at_offset).words
}
