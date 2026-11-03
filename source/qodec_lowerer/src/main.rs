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

use anstream::{ColorChoice, eprintln, print, println};
use qodec::Qodec;
use qodec_runtime::Bit;
use session::{Reply, Session};
use std::{
    io::{self, BufRead, Write},
    process::ExitCode,
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

// Prototype only: `rustyline` is not in the offline cache, so the REPL reads
// plain lines from stdin.
fn repl(session: &mut Session) -> Result<(), io::Error> {
    println!("{}\n", session.banner());
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        print!("qodec> ");
        io::stdout().flush()?;
        let Some(line) = lines.next() else { return Ok(()) };
        let line = line?;
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
