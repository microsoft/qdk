// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;
use std::fmt;
use std::str::FromStr;

use crate::qir_simulation::NoiseTable;

/// Errors that can occur while parsing a noise-table CSV.
#[derive(Debug)]
pub enum ParseError {
    /// A CSV row has an invalid format (wrong number of commas, etc.).
    InvalidRow { line: usize, content: String },
    /// A float value could not be parsed.
    InvalidFloat { line: usize, content: String },
    /// A probability is outside the [0, 1] range.
    InvalidProbability(f64),
    /// Pauli strings within a file have inconsistent lengths.
    InconsistentLength { expected: u32, found: u32 },
    /// A Pauli string contains an invalid character.
    InvalidPauliChar { line: usize, content: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRow { line, content } => {
                write!(f, "invalid csv row in line {line}: `{content}`")
            }
            Self::InvalidFloat { line, content } => {
                write!(f, "invalid float in line {line}: `{content}`")
            }
            Self::InvalidProbability(p) => {
                write!(f, "Probabilities must be in the range [0, 1], found {p}.")
            }
            Self::InconsistentLength { expected, found } => {
                write!(
                    f,
                    "Inconsistent Pauli string length: expected {expected} qubits, found {found}"
                )
            }
            Self::InvalidPauliChar { line, content } => {
                write!(f, "Invalid Pauli string char in line {line}: {content}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl From<ParseError> for pyo3::PyErr {
    fn from(e: ParseError) -> Self {
        use pyo3::exceptions::{PyAttributeError, PyIOError, PyValueError};
        match e {
            ParseError::InvalidRow { .. } | ParseError::InvalidFloat { .. } => {
                PyIOError::new_err(e.to_string())
            }
            ParseError::InvalidProbability(_) | ParseError::InconsistentLength { .. } => {
                PyValueError::new_err(e.to_string())
            }
            ParseError::InvalidPauliChar { .. } => PyAttributeError::new_err(e.to_string()),
        }
    }
}

/// Entries parsed from a single chunk: `(encoded_pauli, probability)` pairs
/// and the qubit count observed in the chunk (if any data lines were present).
type ChunkEntries = (Vec<(u64, f64)>, Option<u32>);

pub fn parse_noise_table(contents: &str) -> Result<NoiseTable, ParseError> {
    use rayon::prelude::*;

    let bytes = contents.as_bytes();
    let num_threads = rayon::current_num_threads().max(1);

    // For small inputs, avoid parallelism overhead.
    if contents.len() < 128 * 1024 || num_threads <= 1 {
        let (entries, qubits) = parse_noise_chunk(contents, 0)?;
        let pauli_noise = FxHashMap::from_iter(entries);
        return Ok(NoiseTable {
            qubits: qubits.unwrap_or(0),
            pauli_noise,
            loss: 0.0,
        });
    }

    // Split the buffer into roughly equal chunks at line boundaries.
    let chunk_size = contents.len() / num_threads;
    let mut boundaries = vec![0usize];
    for i in 1..num_threads {
        let approx = i * chunk_size;
        if approx < contents.len() {
            // Advance past the current (possibly partial) line.
            let end = match memchr::memchr(b'\n', &bytes[approx..]) {
                Some(offset) => approx + offset + 1,
                None => contents.len(),
            };
            if end < contents.len() {
                boundaries.push(end);
            }
        }
    }
    boundaries.push(contents.len());

    // Compute the starting line number for each chunk so error messages
    // report the correct global line number.
    let mut line_offsets = Vec::with_capacity(boundaries.len());
    line_offsets.push(0usize);
    for i in 0..boundaries.len() - 1 {
        let chunk_bytes = &bytes[boundaries[i]..boundaries[i + 1]];
        let nl = memchr::memchr_iter(b'\n', chunk_bytes).count();
        line_offsets.push(line_offsets[i] + nl);
    }

    // Build (slice, line_offset) pairs for each chunk.
    let chunks: Vec<_> = boundaries
        .windows(2)
        .zip(line_offsets.iter())
        .map(|(w, &offset)| (&contents[w[0]..w[1]], offset))
        .collect();

    // Parse all chunks in parallel.
    let chunk_results: Vec<_> = chunks
        .par_iter()
        .map(|&(chunk, line_offset)| parse_noise_chunk(chunk, line_offset))
        .collect::<Result<Vec<_>, _>>()?;

    // Merge: verify consistent qubit counts and insert directly into the map.
    let total_entries: usize = chunk_results.iter().map(|(e, _)| e.len()).sum();
    let mut pauli_noise = FxHashMap::with_capacity_and_hasher(total_entries, Default::default());
    let mut expected_qubits: Option<u32> = None;

    for (entries, chunk_qubits) in chunk_results {
        if let Some(q) = chunk_qubits {
            match expected_qubits {
                None => expected_qubits = Some(q),
                Some(exp) if exp != q => {
                    return Err(ParseError::InconsistentLength {
                        expected: exp,
                        found: q,
                    });
                }
                _ => (),
            }
        }
        for (key, prob) in entries {
            pauli_noise.insert(key, prob);
        }
    }

    let qubits = expected_qubits.unwrap_or(0);

    Ok(NoiseTable {
        qubits,
        pauli_noise,
        loss: 0.0,
    })
}

/// Parse a single chunk of CSV content, returning the non-identity entries
/// and the observed qubit count (if any data lines were present).
/// `line_offset` is the global line number of the first line in this chunk,
/// used for error messages.
fn parse_noise_chunk(contents: &str, line_offset: usize) -> Result<ChunkEntries, ParseError> {
    let capacity = contents.len() / 40;
    let mut entries = Vec::with_capacity(capacity);
    let mut expected_qubits: Option<u32> = None;

    for (local_i, line) in contents.lines().enumerate() {
        let i = line_offset + local_i;

        // Fast skip: check first byte before doing any work.
        if line.is_empty() {
            continue;
        }
        let first = line.as_bytes()[0];
        if first == b'#' || first == b'p' || first == b' ' || first == b'\t' {
            // Full check only for the rare header/comment/whitespace lines.
            if first == b'#' || line.starts_with("pauli") || line.trim().is_empty() {
                continue;
            }
        }

        // --- Inline parse_line + validation + identity check in a single pass ---
        let Some(comma) = memchr::memchr(b',', line.as_bytes()) else {
            return Err(ParseError::InvalidRow {
                line: i,
                content: line.to_string(),
            });
        };

        // Ensure there is no second comma.
        if memchr::memchr(b',', &line.as_bytes()[comma + 1..]).is_some() {
            return Err(ParseError::InvalidRow {
                line: i,
                content: line.to_string(),
            });
        }

        let pauli = line[..comma].trim();
        let prob_str = line[comma + 1..].trim();

        let Ok(prob) = f64::from_str(prob_str) else {
            return Err(ParseError::InvalidFloat {
                line: i,
                content: line.to_string(),
            });
        };
        if !(0.0..=1.0).contains(&prob) {
            return Err(ParseError::InvalidProbability(prob));
        }

        let num_qubits = u32::try_from(pauli.len()).expect("pauli string size should fit in a u32");

        match expected_qubits {
            None => expected_qubits = Some(num_qubits),
            Some(expected) if expected != num_qubits => {
                return Err(ParseError::InconsistentLength {
                    expected,
                    found: num_qubits,
                });
            }
            _ => (),
        }

        // Validate characters, and encode to u64 in one pass.
        let pauli_bytes = pauli.as_bytes();
        let mut key: u64 = 0;
        for &b in pauli_bytes {
            let bits = match b {
                b'I' => 0u64,
                b'X' => 1u64,
                b'Y' => 2u64,
                b'Z' => 3u64,
                _ => {
                    return Err(ParseError::InvalidPauliChar {
                        line: i,
                        content: line.to_string(),
                    });
                }
            };
            key = (key << 2) | bits;
        }

        if key != 0 && prob != 0.0 {
            entries.push((key, prob));
        }
    }

    Ok((entries, expected_qubits))
}
