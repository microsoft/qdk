// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::estimates::{ErrorCorrection, Factory, FactoryBuilder};
use std::borrow::Cow;

/// Implements `Factory` to combine two factories for dispatching
///
/// It requires that all factories define the same code parameter type.
#[derive(Clone)]
pub enum FactoryDispatch2<Factory1, Factory2> {
    Factory1(Factory1),
    Factory2(Factory2),
}

impl<Factory1: Factory, Factory2: Factory<Parameter = Factory1::Parameter>> Factory
    for FactoryDispatch2<Factory1, Factory2>
{
    type Parameter = Factory1::Parameter;

    fn physical_qubits(&self) -> u64 {
        match self {
            Self::Factory1(f) => f.physical_qubits(),
            Self::Factory2(f) => f.physical_qubits(),
        }
    }

    fn duration(&self) -> u64 {
        match self {
            Self::Factory1(f) => f.duration(),
            Self::Factory2(f) => f.duration(),
        }
    }

    fn num_output_states(&self) -> u64 {
        match self {
            Self::Factory1(f) => f.num_output_states(),
            Self::Factory2(f) => f.num_output_states(),
        }
    }

    fn max_code_parameter(&self) -> Option<Cow<'_, Self::Parameter>> {
        match self {
            Self::Factory1(f) => f.max_code_parameter(),
            Self::Factory2(f) => f.max_code_parameter(),
        }
    }
}

/// Implements `FactoryBuilder` to combine two factory builders for dispatching
///
/// It will use `FactoryDispatch2` as factory type for the builder.
pub struct BuilderDispatch2<Builder1, Builder2> {
    builder1: Builder1,
    builder2: Builder2,
}

impl<Builder1: Default, Builder2: Default> Default for BuilderDispatch2<Builder1, Builder2> {
    fn default() -> Self {
        Self {
            builder1: Builder1::default(),
            builder2: Builder2::default(),
        }
    }
}

impl<E: ErrorCorrection, Builder1: FactoryBuilder<E>, Builder2: FactoryBuilder<E>> FactoryBuilder<E>
    for BuilderDispatch2<Builder1, Builder2>
{
    type Factory = FactoryDispatch2<Builder1::Factory, Builder2::Factory>;

    fn find_factories(
        &self,
        ftp: &E,
        qubit: &std::rc::Rc<E::Qubit>,
        magic_state_type: usize,
        output_error_rate: f64,
        max_code_parameter: &E::Parameter,
    ) -> Result<Vec<Cow<'_, Self::Factory>>, String> {
        match magic_state_type {
            0 => self
                .builder1
                .find_factories(
                    ftp,
                    qubit,
                    magic_state_type,
                    output_error_rate,
                    max_code_parameter,
                )
                .map(|factories| {
                    factories
                        .into_iter()
                        .map(|f| Cow::Owned(FactoryDispatch2::Factory1(f.into_owned())))
                        .collect()
                }),
            1 => self
                .builder2
                .find_factories(
                    ftp,
                    qubit,
                    magic_state_type,
                    output_error_rate,
                    max_code_parameter,
                )
                .map(|factories| {
                    factories
                        .into_iter()
                        .map(|f| Cow::Owned(FactoryDispatch2::Factory2(f.into_owned())))
                        .collect()
                }),
            _ => unreachable!("factory builder only has two magic state types"),
        }
    }

    /// Since this builder dispatch is for two factory builders, it can support
    /// two different magic state types.
    fn num_magic_state_types(&self) -> usize {
        2
    }
}
