// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use clap::{Parser, crate_version};
use qsc_formatter::formatter::{calculate_format_edits, format_str};
use std::{
    env,
    fmt::Display,
    path::{Path, PathBuf},
};

#[derive(Debug, Parser)]
#[command(name = "fmt", version = crate_version!())]
#[command(author, about, next_line_help = true)]
struct Cli {
    /// Paths to the files or folders containing files to be formatted.
    #[arg(required = true, num_args = 1..)]
    paths: Vec<PathBuf>,

    /// Search for Q# files recursively when a folder is given as PATH.
    #[arg(short, long, default_value("false"))]
    recursive: bool,

    /// Overwrite the file contents with the formatted contents.
    #[arg(short, long, default_value("false"))]
    write: bool,
}

struct FileWalker {
    roots: Vec<PathBuf>,
    is_write: bool,
    is_recursive: bool,
    file_count: i32,
    changed_files: Vec<String>,
    skipped_files: Vec<String>,
}

impl FileWalker {
    pub fn new() -> Self {
        let cli = Cli::parse();
        Self {
            roots: cli.paths,
            is_write: cli.write,
            is_recursive: cli.recursive,
            file_count: 0,
            changed_files: vec![],
            skipped_files: vec![],
        }
    }

    fn format_from_roots(&mut self) {
        let temp = self.roots.clone();
        for root in temp {
            self.format_file_or_dir(&root);
        }
    }

    fn format_file_or_dir(&mut self, path: &Path) {
        use OutputFormatting::*;

        if path.is_dir() {
            let items = match path.read_dir() {
                Ok(items) => items.flatten(),
                Err(e) => {
                    println!("\t{Skip}Could not read from directory: {e}{Reset}");
                    self.skipped_files.push(path.display().to_string());
                    return;
                }
            };
            for item in items {
                let subpath = &item.path();
                if subpath.is_dir() && self.is_recursive {
                    self.format_file_or_dir(subpath);
                } else if is_path_qs(subpath) {
                    self.format_file(subpath);
                }
            }
        } else if is_path_qs(path) {
            self.format_file(path);
        }
    }

    fn format_file(&mut self, path: &Path) {
        use OutputFormatting::*;

        if self.is_write {
            println!("{Verb}Formatting{Reset} {}", path.display());
        } else {
            println!("{Verb}Checking{Reset} {}", path.display());
        }

        let file_as_string = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                println!("\t{Skip}Could not read from file: {e}{Reset}");
                self.skipped_files.push(path.display().to_string());
                return; // don't count file in file_count
            }
        };
        if self.is_write {
            let formatted = format_str(&file_as_string);
            if file_as_string != formatted {
                match std::fs::write(path, formatted) {
                    Ok(()) => {
                        self.changed_files.push(path.display().to_string());
                    }
                    Err(e) => {
                        println!("\t{Skip}Could not write to file: {e}{Reset}");
                        self.skipped_files.push(path.display().to_string());
                        return; // don't count file in file_count
                    }
                }
            }
        } else if !calculate_format_edits(&file_as_string).is_empty() {
            self.changed_files.push(path.display().to_string());
        }
        self.file_count += 1;
    }
}

enum OutputFormatting {
    Error,
    Skip,
    Passing,
    Verb,
    Reset,
}

impl Display for OutputFormatting {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            OutputFormatting::Error => "\x1B[1;91m", // Bold, Bright Red
            OutputFormatting::Passing => "\x1B[1;92m", // Bold, Bright Green
            OutputFormatting::Skip => "\x1B[1;93m",  // Bold, Bright Yellow
            OutputFormatting::Verb => "\x1B[1;94m",  // Bold, Bright Blue
            OutputFormatting::Reset => "\x1B[0m",
        };
        write!(f, "{output}")
    }
}

#[allow(clippy::too_many_lines)]
fn main() -> Result<(), String> {
    use OutputFormatting::*;

    let mut file_walker = FileWalker::new();

    for root in &file_walker.roots {
        if !root.exists() {
            return Err(format!("Given path {} can not found.", root.display()));
        }
        if !root.is_dir() && !is_path_qs(root) {
            return Err(format!(
                "Given path {} is not a folder or Q# file.",
                root.display()
            ));
        }
    }

    file_walker.format_from_roots();

    println!("Ran against {} files.", file_walker.file_count);

    let are_skipped_files = !file_walker.skipped_files.is_empty();
    let are_changed_files = !file_walker.changed_files.is_empty();

    if file_walker.is_write {
        if are_changed_files {
            println!(
                "{Passing}Updated {} files:",
                file_walker.changed_files.len()
            );
            for f in &file_walker.changed_files {
                println!("\t{Passing}{f}");
            }
            print!("{Reset}");
        } else {
            println!("{Passing}No files updated.{Reset}");
        }
    } else if are_changed_files {
        println!(
            "{Error}{} files are in need of formatting:",
            file_walker.changed_files.len()
        );
        for f in &file_walker.changed_files {
            println!("\t{Error}{f}");
        }
        println!(
            "{Error}Run the formatter with the `--write` option to correct formatting for the above files.{Reset}"
        );
    } else {
        println!(
            "{Passing}{} files are correctly formatted.{Reset}",
            file_walker.file_count
        );
    }
    if are_skipped_files {
        println!("{Skip}Skipped {} files:", file_walker.skipped_files.len());
        for f in &file_walker.skipped_files {
            println!("\t{Skip}{f}");
        }
        print!("{Reset}");
    }

    match (file_walker.is_write, are_changed_files, are_skipped_files) {
        (true, _, false) // writing with no skips
        | (false, false, false) => Ok(()), // checking with all formatted and no skips
        (true, true, true) => {
            println!(
                "{Skip}Updated {} files. {} files skipped.{Reset}",
                file_walker.changed_files.len(),
                file_walker.skipped_files.len()
            );
            Err(format!(
                "Could not read/write from {} files",
                file_walker.skipped_files.len()
            ))
        }
        (true, false, true) => {
            println!(
                "{Skip}No files updated. {} files skipped.{Reset}",
                file_walker.skipped_files.len()
            );
            Err(format!(
                "Could not read/write from {} files",
                file_walker.skipped_files.len()
            ))
        }
        (false, true, true) => {
            println!(
                "{Error}{} files are not formatted. {} files skipped.{Reset}",
                file_walker.changed_files.len(),
                file_walker.skipped_files.len()
            );
            Err(format!(
                "{} files are not formatted and could not read/write from {} files",
                file_walker.changed_files.len(),
                file_walker.skipped_files.len()
            ))
        }
        (false, true, false) => Err(format!(
            "{} files are not formatted",
            file_walker.changed_files.len()
        )),
        (false, false, true) => {
            println!(
                "{Skip}{} files are correctly formatted. {} files skipped.{Reset}",
                file_walker.file_count,
                file_walker.skipped_files.len()
            );
            Err(format!(
                "Could not read/write from {} files",
                file_walker.skipped_files.len()
            ))
        }
    }
}

fn is_path_qs(path: &Path) -> bool {
    if path.is_file()
        && let Some(ex) = path.extension()
        && ex == "qs"
    {
        return true;
    }
    false
}
