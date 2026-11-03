// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `qodec-lowerer [--color=<when>] <qodec-filepath>`: a REPL that lowers
//! instructions of a qodec's top ISA to its bottom ISA, through every layer of
//! the qodec runtime, and prints the bottom instructions. Type `:help` in the
//! REPL for its syntax.
//!
//! Colors are only shown on a color terminal, unless `--color=always`. Output
//! carries the same information without them.

mod highlight;
mod recorder;
mod session;
mod style;
mod syntax;

#[cfg(test)]
mod tests;

use anstream::{AutoStream, ColorChoice, eprintln, print, println};
use qodec::{InstructionSet, Qodec};
use qodec_runtime::Bit;
use rustyline::{
    Config, Context, Editor, Helper,
    completion::Completer,
    config::ColorMode,
    error::ReadlineError,
    highlight::{CmdKind, Highlighter},
    hint::Hinter,
    history::DefaultHistory,
    validate::Validator,
};
use session::{COMMANDS, Reply, Session};
use std::{
    borrow::Cow,
    io::{self, BufRead, Write},
    process::ExitCode,
    sync::Arc,
};

const USAGE: &str = "usage: qodec-lowerer [--color=auto|always|never] <qodec-filepath>";

const HELP: &str = "Lowers top-ISA instructions of a qodec to its bottom ISA.

Options:
  --color <when>  auto (default), always, or never. `auto` colors only a
                  terminal, and honors NO_COLOR and CLICOLOR_FORCE.
  -h, --help      show this help";

/// The parsed command line.
enum Args {
    Help,
    Run { path: String, color: ColorChoice },
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut path = None;
    let mut color = ColorChoice::Auto;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let when = match arg.as_str() {
            "-h" | "--help" => return Ok(Args::Help),
            "--color" => Some(args.next().ok_or("`--color` needs a value")?.as_str()),
            _ => arg.strip_prefix("--color="),
        };
        if let Some(when) = when {
            color = match when {
                "auto" => ColorChoice::Auto,
                "always" => ColorChoice::Always,
                "never" => ColorChoice::Never,
                _ => {
                    return Err(format!(
                        "`--color` expects auto, always or never, got `{when}`"
                    ));
                }
            };
        } else if arg.starts_with('-') {
            return Err(format!("unknown option `{arg}`"));
        } else if path.replace(arg.clone()).is_some() {
            return Err("expected one qodec file".to_string());
        }
    }
    let path = path.ok_or("missing the qodec file")?;
    Ok(Args::Run { path, color })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (path, color) = match parse_args(&args) {
        Ok(Args::Help) => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        Ok(Args::Run { path, color }) => (path, color),
        Err(e) => {
            eprintln!("{} {e}\n{USAGE}", style::error_prefix());
            return ExitCode::FAILURE;
        }
    };
    color.write_global();
    let qodec = match Qodec::load(&path) {
        Ok(qodec) => qodec,
        Err(e) => {
            eprintln!("{} cannot load `{path}`: {e}", style::error_prefix());
            return ExitCode::FAILURE;
        }
    };
    let mut session = match Session::new(&qodec, Box::new(ask)) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("{} cannot plan `{path}`: {e}", style::error_prefix());
            return ExitCode::FAILURE;
        }
    };
    match repl(&mut session) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {e}", style::error_prefix());
            ExitCode::FAILURE
        }
    }
}

fn repl(session: &mut Session) -> Result<(), ReadlineError> {
    // Highlight the input exactly when the output is colored.
    let color_mode = if AutoStream::choice(&io::stdout()) == ColorChoice::Never {
        ColorMode::Disabled
    } else {
        ColorMode::Enabled
    };
    let config = Config::builder().color_mode(color_mode).build();
    let mut editor: Editor<Completions, DefaultHistory> = Editor::with_config(config)?;
    editor.set_helper(Some(Completions(Arc::clone(session.top()))));
    // A blank line after every output separates one exchange from the next.
    println!("{}\n", session.banner());
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
            Reply::Print(text) => println!("{}\n", text.trim_end()),
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

/// Tab-completes and highlights commands, top-ISA mnemonics, and their
/// parameter names.
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
                .map(|c| format!(":{c}"))
                .filter(|c| c.starts_with(word))
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

impl Highlighter for Completions {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        Cow::Owned(highlight::line(&self.0, line))
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        Cow::Owned(style::paint(style::PROMPT, prompt))
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        // Any keystroke can change a token's role.
        true
    }
}

impl Validator for Completions {}

impl Helper for Completions {}
