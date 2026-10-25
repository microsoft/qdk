// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CSV rows a measurement prints: `case,item,value,unit`.
//!
//! `item` is `key=value;…;metric`, without commas; `value` is a number,
//! `true`, `false`, `unavailable` or `error`. An error also prints a comment
//! line, `# case item: message`, and each case ends with an `errors` row,
//! so a sweep that hit engine errors still reports every other point.

use std::{
    fmt::Display,
    time::{Duration, Instant},
};

pub struct Report {
    case: &'static str,
    errors: usize,
}

impl Report {
    /// Starts `case`'s rows on a line of their own: with one test thread,
    /// libtest prints `test name ... ` without a newline before the output.
    pub fn new(case: &'static str) -> Self {
        println!();
        println!("# {case}: case,item,value,unit");
        Self { case, errors: 0 }
    }

    pub fn row(&mut self, item: &str, value: impl Display, unit: &str) {
        println!("{},{item},{value},{unit}", self.case);
    }

    pub fn note(&mut self, item: &str, message: impl Display) {
        let message = message.to_string().replace('\n', " ");
        println!("# {} {item}: {message}", self.case);
    }

    pub fn error(&mut self, item: &str, error: impl Display) {
        self.errors += 1;
        println!("{},{item},error,", self.case);
        self.note(item, error);
    }

    /// The value, or an error row and `None`.
    pub fn record<T, E: Display>(&mut self, item: &str, result: Result<T, E>) -> Option<T> {
        result.map_err(|error| self.error(item, error)).ok()
    }

    /// Runs `operation`, prints its duration in milliseconds, and returns
    /// its value, or an error row and `None`.
    pub fn time<T, E: Display>(
        &mut self,
        item: &str,
        operation: impl FnOnce() -> Result<T, E>,
    ) -> Option<T> {
        let start = Instant::now();
        let result = operation();
        let elapsed = start.elapsed();
        let value = self.record(item, result)?;
        self.row(item, milliseconds(elapsed), "ms");
        Some(value)
    }

    /// Prints the number of error rows; the case's last row.
    pub fn finish(mut self) {
        let errors = self.errors;
        self.row("errors", errors, "count");
    }
}

/// A duration in milliseconds, to the microsecond.
pub fn milliseconds(duration: Duration) -> String {
    format!("{:.3}", duration.as_secs_f64() * 1e3)
}

/// `operation`'s value and how long it took.
pub fn timed<T, E>(operation: impl FnOnce() -> Result<T, E>) -> Result<(T, Duration), E> {
    let start = Instant::now();
    let value = operation()?;
    Ok((value, start.elapsed()))
}

/// The median of the durations `measure` returns over `repeats` runs (at
/// least one), or the first error.
pub fn median<E>(
    repeats: usize,
    mut measure: impl FnMut() -> Result<Duration, E>,
) -> Result<Duration, E> {
    let mut durations = Vec::with_capacity(repeats.max(1));
    for _ in 0..repeats.max(1) {
        durations.push(measure()?);
    }
    durations.sort_unstable();
    Ok(durations[durations.len() / 2])
}

/// A small number, such as an error, in scientific notation.
pub fn scientific(value: f64) -> String {
    format!("{value:.3e}")
}

/// A count or `unavailable`.
pub fn or_unavailable(value: Option<u64>) -> String {
    value.map_or_else(|| "unavailable".to_string(), |value| value.to_string())
}

/// The device memory this process holds, in MiB, as `nvidia-smi` lists its
/// compute processes; `None` when `nvidia-smi` is missing or does not list
/// this process.
pub fn device_mib() -> Option<u64> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-compute-apps=pid,used_memory",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    let pid = std::process::id().to_string();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once(','))
        .filter(|(process, _)| process.trim() == pid)
        .find_map(|(_, used)| used.trim().parse().ok())
}
