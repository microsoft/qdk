// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use num_bigint::BigUint;
use num_complex::{Complex64, ComplexFloat};
use qsc::{
    fmt_basis_state_label, fmt_complex, format_state_id, get_matrix_latex, get_phase,
    get_state_latex,
};
use std::fmt::Write;

#[derive(Clone)]
pub struct DisplayableState(pub Vec<(BigUint, Complex64)>, pub usize);
pub struct DisplayableMatrix(pub Vec<Vec<Complex64>>);

impl DisplayableState {
    pub fn to_plain(&self) -> String {
        if self.1 > 0 {
            format!(
                "STATE:{}",
                self.0
                    .iter()
                    .fold(String::new(), |mut output, (id, state)| {
                        let _ = write!(
                            output,
                            "\n{}: {}",
                            format_state_id(id, self.1),
                            fmt_complex(state)
                        );
                        output
                    })
            )
        } else {
            "STATE:\nNo qubits allocated".to_string()
        }
    }

    pub fn to_html(&self) -> String {
        if self.1 > 0 {
            format!(
                include_str!("state_header_template.html"),
                self.0
                    .iter()
                    .fold(String::new(), |mut output, (id, state)| {
                        let amplitude = state.abs().powi(2) * 100.0;
                        let _ = write!(
                            output,
                            include_str!("state_row_template.html"),
                            fmt_basis_state_label(id, self.1),
                            fmt_complex(state),
                            amplitude,
                            amplitude,
                            get_phase(state),
                            get_phase(state)
                        );
                        output
                    })
            )
        } else {
            format!(
                include_str!("state_header_template.html"),
                "<tr><td>No qubits allocated</td></tr>".to_string()
            )
        }
    }

    pub fn to_latex(&self) -> Option<String> {
        get_state_latex(&self.0, self.1)
    }
}

impl DisplayableMatrix {
    pub fn to_plain(&self) -> String {
        format!(
            "MATRIX:{}",
            self.0.iter().fold(String::new(), |mut output, row| {
                let _ = write!(
                    output,
                    "\n{}",
                    row.iter().fold(String::new(), |mut row_output, element| {
                        let _ = write!(row_output, " {}", fmt_complex(element));
                        row_output
                    })
                );
                output
            })
        )
    }

    pub fn to_latex(&self) -> String {
        get_matrix_latex(&self.0)
    }
}

pub enum DisplayableOutput {
    State(DisplayableState),
    Message(String),
    Matrix(DisplayableMatrix),
}
