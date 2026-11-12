// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This is throw-away code! Its only purpose is to serve as an interactive
//! tool to aid development and reviews.

//! `qodec-hi [--program <file>] <qodec-filepath>`: a terminal workbench for
//! people to build a qodec and see how it lowers a program, layer by layer.
//!
//! `qodec-repl` answers one line at a time, which suits a reader that keeps
//! the whole conversation in mind. This interface keeps everything on screen
//! instead. The text of every file of the qodec is the thing being made, and
//! everything that follows from it is computed again on every keystroke: what
//! each gadget's circuit does, step by step; which of its checks, readouts and
//! frames hold, and which the tool can derive for you; whether each code fits
//! together; and the program's shot, with every call it became at every layer.
//!
//! The runtime is the reference one, unchanged: [`trace::Traced`] wraps its
//! design to watch each layer from the outside. See `DESIGN.md`.

mod analysis;
mod app;
mod bundle;
mod circuit;
mod codes;
mod gf2;
mod shot;
mod stabilizer;
mod state;
mod syntax;
mod trace;
mod ui;
mod views;
mod vocab;
mod workbench;

#[cfg(test)]
mod tests;

use app::App;
use ratatui::{
    DefaultTerminal,
    crossterm::event::{self, Event, KeyEventKind},
};
use std::{io, path::PathBuf, process::ExitCode};

const USAGE: &str = "usage: qodec-hi [--program <file>] <qodec-filepath>";

const HELP: &str = "Build a qodec and see how it lowers a program, layer by layer, in the terminal.

Options:
  --program <file>  the program to start with, one line per batch; Ctrl-S in
                    the interface saves it back, with the qodec's files
  -h, --help        show this help

Press ? in the interface for its keys.";

/// The parsed command line.
enum Args {
    Help,
    Run {
        qodec: String,
        program: Option<PathBuf>,
    },
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut qodec = None;
    let mut program = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Args::Help),
            "--program" => {
                let file = args.next().ok_or("`--program` needs a file")?;
                program = Some(PathBuf::from(file));
            }
            _ if arg.starts_with("--program=") => {
                program = Some(PathBuf::from(&arg["--program=".len()..]));
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option `{arg}`")),
            _ => {
                if qodec.replace(arg.clone()).is_some() {
                    return Err("expected one qodec file".to_string());
                }
            }
        }
    }
    let qodec = qodec.ok_or("missing the qodec file")?;
    Ok(Args::Run { qodec, program })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (path, program) = match parse_args(&args) {
        Ok(Args::Help) => {
            println!("{USAGE}\n\n{HELP}");
            return ExitCode::SUCCESS;
        }
        Ok(Args::Run { qodec, program }) => (qodec, program),
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    let docs = match bundle::load(std::path::Path::new(&path)) {
        Ok(docs) => docs,
        Err(e) => {
            eprintln!("error: cannot read `{path}`: {e}");
            return ExitCode::FAILURE;
        }
    };
    let texts: Vec<String> = match &program {
        Some(file) if file.exists() => match std::fs::read_to_string(file) {
            Ok(text) => text.lines().map(str::to_string).collect(),
            Err(e) => {
                eprintln!("error: cannot read `{}`: {e}", file.display());
                return ExitCode::FAILURE;
            }
        },
        _ => Vec::new(),
    };
    let (docs, source) = docs;
    let mut app = match App::open(docs, Some(source), &texts, program) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("error: cannot load `{path}`: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|frame| ui::render(frame, app))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.key(key);
        }
    }
    Ok(())
}
