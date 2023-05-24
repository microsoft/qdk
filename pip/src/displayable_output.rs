// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use num_bigint::BigUint;
use num_complex::{Complex, Complex64, ComplexFloat};

pub struct DisplayableState(pub Vec<(BigUint, Complex64)>, pub usize);

impl DisplayableState {
    pub fn to_plain(&self) -> String {
        format!(
            "STATE:{}",
            self.0
                .iter()
                .map(|(id, state)| format!(
                    "\n|{}⟩: {}",
                    Self::fmt_basis_state_label(id, self.1),
                    Self::fmt_complex(state)
                ))
                .collect::<String>()
        )
    }

    pub fn to_html(&self) -> String {
        format!(
            include_str!("state_header_template.html"),
            self.0
                .iter()
                .map(|(id, state)| {
                    let amplitude = state.abs().powi(2) * 100.0;
                    format!(
                        include_str!("state_row_template.html"),
                        Self::fmt_basis_state_label(id, self.1),
                        Self::fmt_complex(state),
                        amplitude,
                        amplitude,
                        Self::phase(state),
                        Self::phase(state)
                    )
                })
                .collect::<String>()
        )
    }

    fn phase(c: &Complex<f64>) -> f64 {
        f64::atan2(c.im, c.re)
    }

    fn fmt_complex(c: &Complex<f64>) -> String {
        // Format -0 as 0
        // Also using Unicode Minus Sign instead of ASCII Hyphen-Minus
        // and Unicode Mathematical Italic Small I instead of ASCII i.
        format!(
            "{}{:.4}{}{:.4}𝑖",
            if c.re <= -0.00005 { "−" } else { "" },
            c.re.abs(),
            if c.im <= -0.00005 { "−" } else { "+" },
            c.im.abs()
        )
    }

    fn fmt_basis_state_label(id: &BigUint, num_qubits: usize) -> String {
        // This will generate a bit string that shows the qubits in the order
        // of allocation, left to right.
        format!("{:0>width$}", id.to_str_radix(2), width = num_qubits)
    }
}

pub enum DisplayableOutput {
    State(DisplayableState),
    Message(String),
}
