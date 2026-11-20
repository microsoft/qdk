// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Highlights the line being typed. It only adds styles, never characters, so
//! the line keeps its display width, and it tolerates half-typed input.

use crate::{
    session::{COMMAND_ALIASES, COMMANDS},
    style::{BLOCK, COMMAND, DIM, INVALID, MNEMONIC, PARAM, VALUE, paint},
};
use qodec::{Instruction, InstructionSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Space,
    Semicolon,
    Word,
}

/// Splits `line` into runs of whitespace, `;`s and words, keeping quoted text
/// and brackets inside words, like the parser does.
fn pieces(line: &str) -> Vec<(Kind, &str)> {
    let mut pieces: Vec<(Kind, &str)> = Vec::new();
    let (mut quoted, mut depth) = (false, 0usize);
    let mut start = 0;
    let mut current = None;
    for (i, c) in line.char_indices() {
        let free = !quoted && depth == 0;
        let kind = if free && c == ';' {
            Kind::Semicolon
        } else if free && c.is_whitespace() {
            Kind::Space
        } else {
            Kind::Word
        };
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => depth += 1,
            ']' if !quoted => depth = depth.saturating_sub(1),
            _ => {}
        }
        if let Some(previous) = current
            && (previous != kind || kind == Kind::Semicolon)
        {
            pieces.push((previous, &line[start..i]));
            start = i;
        }
        current = Some(kind);
    }
    if let Some(kind) = current {
        pieces.push((kind, &line[start..]));
    }
    pieces
}

/// `line`, highlighted against the top ISA `isa`.
pub fn line(isa: &InstructionSet, line: &str) -> String {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return paint(DIM, line);
    }
    let is_command = trimmed.starts_with(':');
    let mut text = String::with_capacity(line.len() * 2);
    // The instruction of the current statement, once its mnemonic is known.
    let mut instruction: Option<&Instruction> = None;
    let mut words = 0;
    for (kind, piece) in pieces(line) {
        match kind {
            Kind::Space => text.push_str(piece),
            Kind::Semicolon if !is_command => {
                text.push_str(&paint(DIM, piece));
                instruction = None;
                words = 0;
            }
            Kind::Semicolon | Kind::Word => {
                text.push_str(&if is_command {
                    command_word(words, piece)
                } else {
                    instruction_word(isa, &mut instruction, words, piece)
                });
                words += 1;
            }
        }
    }
    text
}

fn command_word(index: usize, word: &str) -> String {
    if index > 0 {
        return paint(VALUE, word);
    }
    let name = word.trim_start_matches(':');
    let names = || COMMANDS.iter().chain(COMMAND_ALIASES);
    if names().any(|c| *c == name) {
        paint(COMMAND, word)
    } else if names().any(|c| c.starts_with(name)) {
        word.to_string()
    } else {
        paint(INVALID, word)
    }
}

fn instruction_word<'a>(
    isa: &'a InstructionSet,
    instruction: &mut Option<&'a Instruction>,
    index: usize,
    word: &str,
) -> String {
    if index == 0 {
        *instruction = isa.instructions.iter().find(|i| i.mnemonic == word);
        return if instruction.is_some() {
            paint(MNEMONIC, word)
        } else if isa
            .instructions
            .iter()
            .any(|i| i.mnemonic.starts_with(word))
        {
            word.to_string()
        } else {
            paint(INVALID, word)
        };
    }
    let names = |prefix: &str| {
        instruction.is_some_and(|i| i.parameters.iter().any(|p| p.name.starts_with(prefix)))
    };
    if let Some((name, value)) = word.split_once('=') {
        let name = match instruction {
            Some(i) if i.parameters.iter().any(|p| p.name == name) => paint(PARAM, name),
            Some(_) => paint(INVALID, name),
            None => name.to_string(),
        };
        format!("{name}={}", paint(VALUE, value))
    } else if word.bytes().all(|b| b.is_ascii_digit()) {
        paint(BLOCK, word)
    } else if instruction.is_none() || names(word) {
        // Possibly a parameter name, before its `=`.
        word.to_string()
    } else {
        paint(INVALID, word)
    }
}
