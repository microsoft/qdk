// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `qodec-lowerer <qodec-filepath>`: a REPL that lowers instructions of a
//! qodec's top ISA to its bottom ISA, through every layer of the qodec runtime,
//! and prints the bottom instructions. Type `:help` in the REPL for its syntax.

mod recorder;
mod session;
mod syntax;

#[cfg(test)]
mod tests;

use qodec::{InstructionSet, Qodec};
use qodec_runtime::Bit;
use rustyline::{
    Context, Editor, Helper, completion::Completer, error::ReadlineError, highlight::Highlighter,
    hint::Hinter, history::DefaultHistory, validate::Validator,
};
use session::{Reply, Session};
use std::{
    io::{self, BufRead, Write},
    process::ExitCode,
    sync::Arc,
};

const USAGE: &str = "usage: qodec-lowerer <qodec-filepath>";

const COMMANDS: &[&str] = &[":help", ":isa", ":layers", ":outcomes", ":reset", ":quit"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{USAGE}\n\nLowers top-ISA instructions of a qodec to its bottom ISA.");
            return ExitCode::SUCCESS;
        }
        [path] => path,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    let qodec = match Qodec::load(path) {
        Ok(qodec) => qodec,
        Err(e) => {
            eprintln!("error: cannot load `{path}`: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut session = match Session::new(&qodec, Box::new(ask)) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("error: cannot plan `{path}`: {e}");
            return ExitCode::FAILURE;
        }
    };
    match repl(&mut session) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn repl(session: &mut Session) -> Result<(), ReadlineError> {
    let mut editor: Editor<Completions, DefaultHistory> = Editor::new()?;
    editor.set_helper(Some(Completions(Arc::clone(session.top()))));
    println!("{}", session.banner());
    loop {
        let line = match editor.readline("qodec> ") {
            Ok(line) => line,
            // Ctrl-C clears the line, like a shell.
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => return Ok(()),
            Err(e) => return Err(e),
        };
        if !line.trim().is_empty() {
            editor.add_history_entry(line.as_str())?;
        }
        match session.eval(&line) {
            Reply::Print(text) if text.is_empty() => {}
            Reply::Print(text) => println!("{}", text.trim_end()),
            Reply::Quit => return Ok(()),
        }
    }
}

/// Asks for one bit in `:outcomes ask` mode.
fn ask(what: &str) -> Result<Bit, String> {
    let mut stdin = io::stdin().lock();
    loop {
        print!("  {what}? [0/1/?] ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        let mut answer = String::new();
        if stdin.read_line(&mut answer).map_err(|e| e.to_string())? == 0 {
            return Err("no answer: end of input".to_string());
        }
        match answer.trim() {
            "0" => return Ok(Some(false)),
            "1" => return Ok(Some(true)),
            "?" => return Ok(None),
            _ => println!("  answer 0, 1, or ? for unresolved"),
        }
    }
}

/// Tab-completes commands, top-ISA mnemonics, and their parameter names.
struct Completions(Arc<InstructionSet>);

impl Completer for Completions {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        let before = &line[..pos];
        let statement_start = before.rfind(';').map_or(0, |i| i + 1);
        let word_start = before
            .rfind(char::is_whitespace)
            .map_or(0, |i| i + 1)
            .max(statement_start);
        let word = &before[word_start..];
        let statement: Vec<&str> = before[statement_start..word_start]
            .split_whitespace()
            .collect();
        let candidates: Vec<String> = match statement.first() {
            None if word.starts_with(':') => COMMANDS
                .iter()
                .filter(|c| c.starts_with(word))
                .map(ToString::to_string)
                .collect(),
            None => self
                .0
                .instructions
                .iter()
                .map(|i| i.mnemonic.clone())
                .filter(|m| m.starts_with(word))
                .collect(),
            Some(mnemonic) => self
                .0
                .instructions
                .iter()
                .find(|i| i.mnemonic == *mnemonic)
                .map(|i| {
                    i.parameters
                        .iter()
                        .map(|p| format!("{}=", p.name))
                        .filter(|p| p.starts_with(word))
                        .collect()
                })
                .unwrap_or_default(),
        };
        Ok((word_start, candidates))
    }
}

impl Hinter for Completions {
    type Hint = String;
}

impl Highlighter for Completions {}

impl Validator for Completions {}

impl Helper for Completions {}
