// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::io::{Cursor, Write};

use crate::state::{fmt_complex, format_state_id};
use num_bigint::BigUint;
use num_complex::Complex64;

#[derive(Copy, Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Error;

pub trait Receiver {
    /// Receive state output
    /// # Errors
    /// This will return an error if handling the output fails.
    fn state(&mut self, state: Vec<(BigUint, Complex64)>, qubit_count: usize) -> Result<(), Error>;

    /// Receive matrix output
    /// # Errors
    /// This will return an error if handling the output fails.
    fn matrix(&mut self, matrix: Vec<Vec<Complex64>>) -> Result<(), Error>;

    /// Receive generic message output
    /// # Errors
    /// This will return an error if handling the output fails.
    fn message(&mut self, msg: &str) -> Result<(), Error>;
}

/// Receiver that stores everything it receives.
#[derive(Debug, Default)]
pub struct StoringReceiver {
    pub states: Vec<(Vec<(BigUint, Complex64)>, usize)>,
    pub matrices: Vec<Vec<Vec<Complex64>>>,
    pub messages: Vec<String>,
}

impl StoringReceiver {
    #[must_use]
    pub fn last_matrix(&self) -> &[Vec<Complex64>] {
        self.matrices
            .last()
            .map(Vec::as_slice)
            .expect("expected matrix")
    }
}

impl Receiver for StoringReceiver {
    fn state(&mut self, state: Vec<(BigUint, Complex64)>, qubit_count: usize) -> Result<(), Error> {
        self.states.push((state, qubit_count));
        Ok(())
    }

    fn matrix(&mut self, matrix: Vec<Vec<Complex64>>) -> Result<(), Error> {
        self.matrices.push(matrix);
        Ok(())
    }

    fn message(&mut self, msg: &str) -> Result<(), Error> {
        self.messages.push(msg.to_owned());
        Ok(())
    }
}

pub struct GenericReceiver<'a> {
    writer: &'a mut dyn Write,
}

impl<'a> GenericReceiver<'a> {
    pub fn new(writer: &'a mut impl Write) -> Self {
        Self { writer }
    }
}

impl Receiver for GenericReceiver<'_> {
    fn state(&mut self, state: Vec<(BigUint, Complex64)>, qubit_count: usize) -> Result<(), Error> {
        writeln!(self.writer, "STATE:").map_err(|_| Error)?;
        if qubit_count > 0 {
            for (id, state) in state {
                writeln!(
                    self.writer,
                    "{}: {}",
                    format_state_id(&id, qubit_count),
                    fmt_complex(&state),
                )
                .map_err(|_| Error)?;
            }
        } else {
            writeln!(self.writer, "No qubits allocated").map_err(|_| Error)?;
        }
        Ok(())
    }

    fn matrix(&mut self, matrix: Vec<Vec<Complex64>>) -> Result<(), Error> {
        writeln!(self.writer, "MATRIX:").map_err(|_| Error)?;
        for row in matrix {
            let row_str = row.iter().map(fmt_complex).collect::<Vec<_>>().join(" ");
            writeln!(self.writer, "{row_str}").map_err(|_| Error)?;
        }
        Ok(())
    }

    fn message(&mut self, msg: &str) -> Result<(), Error> {
        writeln!(self.writer, "{msg}").map_err(|_| Error)
    }
}

pub struct CursorReceiver<'a> {
    cursor: &'a mut Cursor<Vec<u8>>,
}

impl<'a> CursorReceiver<'a> {
    pub fn new(cursor: &'a mut Cursor<Vec<u8>>) -> Self {
        Self { cursor }
    }
    pub fn dump(&mut self) -> String {
        let v = self.cursor.get_mut();
        let s = match std::str::from_utf8(v) {
            Ok(v) => v.to_owned(),
            Err(e) => format!("Invalid UTF-8 sequence: {e}"),
        };
        v.clear();
        s.trim().to_string()
    }
}

impl Receiver for CursorReceiver<'_> {
    fn state(&mut self, state: Vec<(BigUint, Complex64)>, qubit_count: usize) -> Result<(), Error> {
        writeln!(self.cursor, "STATE:").map_err(|_| Error)?;
        if qubit_count > 0 {
            for (id, state) in state {
                writeln!(
                    self.cursor,
                    "{}: {}",
                    format_state_id(&id, qubit_count),
                    state
                )
                .map_err(|_| Error)?;
            }
        } else {
            writeln!(self.cursor, "No qubits allocated").map_err(|_| Error)?;
        }
        Ok(())
    }

    fn matrix(&mut self, matrix: Vec<Vec<Complex64>>) -> Result<(), Error> {
        writeln!(self.cursor, "MATRIX:").map_err(|_| Error)?;
        for row in matrix {
            let row_str = row.iter().map(fmt_complex).collect::<Vec<_>>().join(" ");
            writeln!(self.cursor, "{row_str}").map_err(|_| Error)?;
        }
        Ok(())
    }

    fn message(&mut self, msg: &str) -> Result<(), Error> {
        writeln!(self.cursor, "{msg}").map_err(|_| Error)
    }
}

#[cfg(test)]
mod tests {
    use super::{Receiver, StoringReceiver};
    use num_bigint::BigUint;
    use num_complex::Complex64;

    #[test]
    fn storing_receiver_records_all_output() {
        let state = vec![(BigUint::from(1_u8), Complex64::new(0.0, 1.0))];
        let matrix = vec![vec![Complex64::new(1.0, 0.0)]];
        let mut receiver = StoringReceiver::default();

        receiver
            .state(state.clone(), 1)
            .expect("storing state should succeed");
        receiver
            .matrix(matrix.clone())
            .expect("storing matrix should succeed");
        receiver
            .message("message")
            .expect("storing message should succeed");

        assert_eq!(receiver.states, vec![(state, 1)]);
        assert_eq!(receiver.matrices, vec![matrix]);
        assert_eq!(receiver.messages, vec!["message"]);
        assert_eq!(receiver.last_matrix(), receiver.matrices[0].as_slice());
    }
}
