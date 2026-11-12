// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How a read of the computed MPS is answered: one native expectation, whose
//! objects are released before it returns, divided by the squared norm the
//! library reports beside it, which is checked first.

use super::{NativeState, mode};
use crate::engine::{
    EngineError,
    error::{combine_execution_and_cleanup, first_failure},
    native_calls::{OpaqueHandle, complex::Complex64Abi},
};
use num_complex::Complex64;
use std::{collections::BTreeMap, rc::Rc};

impl NativeState {
    /// Evaluates `Σₖ cₖ⟨ψ|Pₖ|ψ⟩` and `⟨ψ|ψ⟩` on the computed state in one
    /// expectation, whose objects are released before it returns.
    ///
    /// Each factor is a one-mode tensor. For one mode, `AppendProduct`'s
    /// forward mode order and `ApplyTensorOperator`'s reversed order
    /// coincide, so the textbook row-major matrix is read as `⟨i|P|j⟩`;
    /// this matters for Y, the only non-symmetric Pauli.
    pub(super) fn query(
        &mut self,
        terms: &[ProductTerm],
    ) -> Result<(Complex64, Complex64), EngineError> {
        let started = std::time::Instant::now();
        let mark = self.allocations.len();
        let query = self.open_query(terms);
        let release = self.release_query(mark);
        self.device_session
            .record_dev_time(|times| times.expectation.add_since(started));
        combine_execution_and_cleanup(query, release)
    }

    /// The probability that `site` reads One when `one` is true, Zero when
    /// it is false, in the computed state, clamped to [0, 1] against
    /// rounding.
    pub(super) fn probability(&mut self, site: usize, one: bool) -> Result<f64, EngineError> {
        let mut diagonal = [0.0, 0.0];
        diagonal[usize::from(one)] = 1.0;
        let projector = BTreeMap::from([(mode(site)?, diagonal)]);
        let (value, squared_norm) = self.query(&[diagonal_term(&projector)])?;
        Ok(normalize_expectation(value, squared_norm)?
            .re
            .clamp(0.0, 1.0))
    }

    fn open_query(&mut self, terms: &[ProductTerm]) -> Result<(Complex64, Complex64), EngineError> {
        let device_session = Rc::clone(&self.device_session);
        let (calls, handle, stream) = (
            device_session.calls(),
            self.handle(),
            device_session.stream(),
        );
        let operator = calls.create_network_operator(handle, self.state_extents.as_ref())?;
        self.network_operator = Some(operator);
        // One device tensor per distinct factor matrix, shared by every
        // factor that uses it.
        let mut tensors: Vec<([Complex64Abi; 4], OpaqueHandle)> = Vec::new();
        for term in terms {
            let mut factor_modes = Vec::with_capacity(term.factors.len());
            let mut factor_tensors = Vec::with_capacity(term.factors.len());
            for (mode, matrix) in &term.factors {
                let tensor = if let Some((_, tensor)) = tensors.iter().find(|(m, _)| m == matrix) {
                    *tensor
                } else {
                    let tensor = self.allocate_complex(matrix.len(), "expectation factor")?;
                    calls.copy_to_device(tensor, matrix)?;
                    tensors.push((*matrix, tensor));
                    tensor
                };
                factor_modes.push(vec![*mode].into_boxed_slice());
                factor_tensors.push(tensor);
            }
            calls.append_product(
                handle,
                operator,
                term.coefficient,
                &factor_modes,
                &factor_tensors,
            )?;
        }
        let expectation = calls.create_expectation(handle, self.state()?, operator)?;
        self.expectation = Some(expectation);
        calls.configure_expectation_hyper_samples(
            handle,
            expectation,
            device_session.dev_tuning().expectation_hyper_samples,
        )?;
        let query_workspace = calls.create_workspace(handle)?;
        self.query_workspace = Some(query_workspace);
        let available = self.available_workspace_bytes()?;
        calls.prepare_expectation(handle, expectation, available, query_workspace, stream)?;
        let bytes = calls.workspace_size(handle, query_workspace)?;
        if bytes < 0 {
            return Err(EngineError::InvalidNativeResult {
                reason: format!("the expectation's workspace size is {bytes}"),
            });
        }
        if bytes > 0 {
            self.attach_scratch(query_workspace, bytes, "expectation workspace")?;
        }
        calls.synchronize_copies()?;
        let values = calls.compute_expectation(handle, expectation, query_workspace, stream)?;
        calls.synchronize_stream(stream)?;
        Ok(values)
    }

    /// Closes the expectation's objects, then frees the buffers allocated
    /// since `mark`.
    fn release_query(&mut self, mark: usize) -> Result<(), EngineError> {
        let closed = self.close_query_lifecycle();
        let freed = self.free_since(mark);
        closed.and(freed)
    }

    pub(super) fn close_query_lifecycle(&mut self) -> Result<(), EngineError> {
        let calls = self.device_session.calls();
        first_failure(
            [
                self.query_workspace
                    .take()
                    .map(|workspace| calls.destroy_workspace(workspace)),
                self.expectation
                    .take()
                    .map(|expectation| calls.destroy_expectation(expectation)),
                self.network_operator
                    .take()
                    .map(|operator| calls.destroy_network_operator(operator)),
            ]
            .into_iter()
            .flatten(),
        )
    }

    /// Frees the allocations made since `mark`, newest first, once the
    /// stream has finished with them; attempts every free and returns the
    /// first failure.
    fn free_since(&mut self, mark: usize) -> Result<(), EngineError> {
        let device_session = Rc::clone(&self.device_session);
        let calls = device_session.calls();
        let synchronized = calls.synchronize_stream(device_session.stream());
        let since_mark = mark.min(self.allocations.len());
        let drained: Vec<_> = self.allocations.drain(since_mark..).rev().collect();
        let freed: Vec<_> = drained
            .into_iter()
            .map(|allocation| self.release_buffer(allocation))
            .collect();
        first_failure(std::iter::once(synchronized).chain(freed))
    }
}

/// One term of an expectation's operator: a coefficient times a product of
/// one-site factors.
pub(super) struct ProductTerm {
    pub(super) coefficient: Complex64,
    pub(super) factors: Vec<(i32, [Complex64Abi; 4])>,
}

/// The term, of coefficient one, that is the product of the one-site
/// diagonal operators `diagonals`.
pub(super) fn diagonal_term(diagonals: &BTreeMap<i32, [f64; 2]>) -> ProductTerm {
    let zero = Complex64Abi::new(0.0, 0.0);
    ProductTerm {
        coefficient: Complex64::new(1.0, 0.0),
        factors: diagonals
            .iter()
            .map(|(&mode, &[d0, d1])| {
                (
                    mode,
                    [
                        Complex64Abi::new(d0, 0.0),
                        zero,
                        zero,
                        Complex64Abi::new(d1, 0.0),
                    ],
                )
            })
            .collect(),
    }
}

/// `⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩`: a truncated MPS is not normalized, so every read
/// divides by the norm the library reports beside it, and rejects a
/// non-finite value or a norm `positive_squared_norm` rejects.
pub(super) fn normalize_expectation(
    raw_expectation: Complex64,
    squared_norm: Complex64,
) -> Result<Complex64, EngineError> {
    let finite = |value: Complex64| value.re.is_finite() && value.im.is_finite();
    let invalid = |reason: &str| EngineError::InvalidNativeResult {
        reason: reason.to_string(),
    };
    if !finite(raw_expectation) {
        return Err(invalid("the expectation returned a non-finite value"));
    }
    let normalized = raw_expectation / positive_squared_norm(squared_norm)?;
    if finite(normalized) {
        Ok(normalized)
    } else {
        Err(invalid("the normalized expectation is non-finite"))
    }
}

/// The squared norm the library reported beside an expectation, which must
/// be finite, real to rounding and positive: every read divides by it, and
/// it is the retained weight, which the contract keeps above zero.
pub(super) fn positive_squared_norm(squared_norm: Complex64) -> Result<f64, EngineError> {
    let invalid = |reason: &str| EngineError::InvalidNativeResult {
        reason: reason.to_string(),
    };
    if !(squared_norm.re.is_finite() && squared_norm.im.is_finite()) {
        return Err(invalid(
            "the expectation returned a non-finite squared norm",
        ));
    }
    if squared_norm.im.abs() > 1.0e-12 * squared_norm.re.abs().max(1.0) {
        return Err(invalid(
            "the expectation's squared norm has a material imaginary part",
        ));
    }
    if squared_norm.re <= 0.0 {
        return Err(invalid(
            "the expectation returned a nonpositive squared norm",
        ));
    }
    Ok(squared_norm.re)
}
