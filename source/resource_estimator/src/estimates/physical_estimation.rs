// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod estimate_frontier;
mod estimate_without_restrictions;
mod result;

use super::{
    Error, ErrorBudget, ErrorBudgetStrategy, ErrorCorrection, Factory, FactoryBuilder,
    LogicalPatch, Overhead,
};
use std::{borrow::Cow, rc::Rc};

use estimate_frontier::EstimateFrontier;
use estimate_without_restrictions::EstimateWithoutRestrictions;
pub use result::{FactoryPart, PhysicalResourceEstimationResult};

pub struct PhysicalResourceEstimation<E: ErrorCorrection, Builder, L> {
    // required parameters
    ftp: E,
    qubit: Rc<E::Qubit>,
    factory_builder: Builder,
    layout_overhead: Rc<L>,
    // optional constraint parameters
    logical_depth_factor: Option<f64>,
    max_factories: Option<u64>,
    max_duration: Option<u64>,
    max_physical_qubits: Option<u64>,
    error_budget_strategy: ErrorBudgetStrategy,
}

impl<
    E: ErrorCorrection<Parameter = impl Clone>,
    Builder: FactoryBuilder<E, Factory = impl Factory<Parameter = E::Parameter> + Clone>,
    L: Overhead,
> PhysicalResourceEstimation<E, Builder, L>
{
    pub fn new(
        ftp: E,
        qubit: Rc<E::Qubit>,
        factory_builder: Builder,
        layout_overhead: Rc<L>,
    ) -> Self {
        Self {
            ftp,
            qubit,
            factory_builder,
            layout_overhead,
            logical_depth_factor: None,
            max_factories: None,
            max_duration: None,
            max_physical_qubits: None,
            error_budget_strategy: ErrorBudgetStrategy::default(),
        }
    }

    pub fn error_correction(&self) -> &E {
        &self.ftp
    }

    pub fn layout_overhead(&self) -> &L {
        &self.layout_overhead
    }

    pub fn set_logical_depth_factor(&mut self, logical_depth_factor: f64) {
        self.logical_depth_factor = Some(logical_depth_factor);
    }
    pub fn set_max_factories(&mut self, max_factories: u64) {
        self.max_factories = Some(max_factories);
    }
    pub fn set_max_duration(&mut self, max_duration: u64) {
        self.max_duration = Some(max_duration);
    }
    pub fn set_max_physical_qubits(&mut self, max_physical_qubits: u64) {
        self.max_physical_qubits = Some(max_physical_qubits);
    }

    pub fn error_budget_strategy(&self) -> ErrorBudgetStrategy {
        self.error_budget_strategy
    }

    pub fn set_error_budget_strategy(&mut self, error_budget_strategy: ErrorBudgetStrategy) {
        self.error_budget_strategy = error_budget_strategy;
    }

    pub fn factory_builder(&self) -> &Builder {
        &self.factory_builder
    }

    pub fn factory_builder_mut(&mut self) -> &mut Builder {
        &mut self.factory_builder
    }

    pub fn estimate(
        &self,
        error_budget: &ErrorBudget,
    ) -> Result<PhysicalResourceEstimationResult<E, Builder::Factory>, Error> {
        match (self.max_duration, self.max_physical_qubits) {
            (None, None) => self.estimate_without_restrictions(error_budget),
            (None, Some(max_physical_qubits)) => {
                self.estimate_with_max_num_qubits(error_budget, max_physical_qubits)
            }
            (Some(max_duration), None) => {
                self.estimate_with_max_duration(error_budget, max_duration)
            }
            _ => Err(Error::BothDurationAndPhysicalQubitsProvided),
        }
    }

    pub fn build_frontier(
        &self,
        error_budget: &ErrorBudget,
    ) -> Result<Vec<PhysicalResourceEstimationResult<E, Builder::Factory>>, Error> {
        EstimateFrontier::new(self, error_budget)?.estimate()
    }

    pub fn estimate_without_restrictions(
        &self,
        error_budget: &ErrorBudget,
    ) -> Result<PhysicalResourceEstimationResult<E, Builder::Factory>, Error> {
        EstimateWithoutRestrictions::new(self).estimate(error_budget)
    }

    fn compute_initial_optimization_values(
        &self,
        error_budget: &ErrorBudget,
    ) -> Result<InitialOptimizationValues<E::Parameter>, Error> {
        let num_cycles_required_by_layout_overhead = self.compute_num_cycles(error_budget)?;

        // The required magic state error rate is computed by dividing the total
        // error budget for magic states by the number of magic states required
        // for the algorithm.
        let required_logical_magic_state_error_rate = error_budget.magic_states()
            / (self
                .layout_overhead
                .num_magic_states(error_budget, 0)
                .map_err(Error::NumberOfMagicStatesComputationFailed)? as f64);

        let required_logical_error_rate = self.required_logical_error_rate(
            error_budget.logical(),
            num_cycles_required_by_layout_overhead,
        )?;

        let min_code_parameter = self.compute_code_parameter(required_logical_error_rate)?;

        Ok(InitialOptimizationValues {
            min_code_parameter,
            num_cycles_required_by_layout_overhead,
            required_logical_error_rate,
            required_logical_magic_state_error_rate,
        })
    }

    #[allow(clippy::too_many_lines)]
    pub fn estimate_with_max_duration(
        &self,
        error_budget: &ErrorBudget,
        max_duration_in_nanoseconds: u64,
    ) -> Result<PhysicalResourceEstimationResult<E, Builder::Factory>, Error> {
        if self.factory_builder.num_magic_state_types() != 1 {
            return Err(Error::MultipleMagicStatesNotSupported);
        }

        let InitialOptimizationValues {
            min_code_parameter,
            num_cycles_required_by_layout_overhead,
            required_logical_error_rate,
            required_logical_magic_state_error_rate,
        } = self.compute_initial_optimization_values(error_budget)?;

        let num_magic_states = self
            .layout_overhead
            .num_magic_states(error_budget, 0)
            .map_err(Error::NumberOfMagicStatesComputationFailed)?;
        if num_magic_states == 0 {
            let logical_patch =
                LogicalPatch::new(&self.ftp, min_code_parameter, self.qubit.clone())?;

            if num_cycles_required_by_layout_overhead * logical_patch.logical_cycle_time()
                <= max_duration_in_nanoseconds
            {
                return PhysicalResourceEstimationResult::without_factories(
                    self,
                    logical_patch,
                    error_budget,
                    num_cycles_required_by_layout_overhead,
                    required_logical_error_rate,
                );
            }
            return Err(Error::MaxDurationTooSmall);
        }

        let mut best_estimation_result: Option<
            PhysicalResourceEstimationResult<E, Builder::Factory>,
        > = None;

        let mut last_factories = Vec::new();
        let mut last_code_parameter = None;

        for code_parameter in self
            .ftp
            .code_parameter_range(Some(&min_code_parameter))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let logical_patch =
                LogicalPatch::new(&self.ftp, code_parameter.clone(), self.qubit.clone())?;

            let max_num_cycles_allowed_by_duration = ((max_duration_in_nanoseconds as f64)
                / logical_patch.logical_cycle_time() as f64)
                .floor() as u64;
            if max_num_cycles_allowed_by_duration < num_cycles_required_by_layout_overhead {
                continue;
            }

            let max_num_cycles_allowed_by_error_rate =
                self.logical_cycles_for_code_parameter(error_budget.logical(), &code_parameter)?;

            if max_num_cycles_allowed_by_error_rate < num_cycles_required_by_layout_overhead {
                continue;
            }

            let max_num_cycles_allowed =
                max_num_cycles_allowed_by_duration.min(max_num_cycles_allowed_by_error_rate);

            // The initial value for the last code parameter is `None`. This
            // ensures that the first code parameter is always tried. After
            // that, the last code parameter governs the reuse of the magic
            // state factory.
            if last_code_parameter.as_ref().is_none_or(|d| {
                self.ftp
                    .code_parameter_cmp(self.qubit.as_ref(), d, &code_parameter)
                    .is_gt()
            }) {
                last_factories = self
                    .factory_builder
                    .find_factories(
                        &self.ftp,
                        &self.qubit,
                        0,
                        required_logical_magic_state_error_rate,
                        &code_parameter,
                    )
                    .map_err(Error::FactorySearchFailed)?;

                last_code_parameter = self.find_highest_code_parameter(&last_factories);
            }

            for FactoryForCycles { factory, .. } in Self::pick_factories_with_num_cycles(
                &last_factories,
                &logical_patch,
                max_num_cycles_allowed,
            ) {
                let num_factories = self.num_factories(
                    &logical_patch,
                    0,
                    &factory,
                    error_budget,
                    max_num_cycles_allowed,
                )?;

                let num_cycles_required_for_magic_states = self
                    .compute_num_cycles_required_for_magic_states(
                        0,
                        num_factories,
                        &factory,
                        &logical_patch,
                        error_budget,
                    )?;

                // This num_cycles could be larger than num_cycles_required_by_layout_overhead
                // but must still not exceed the maximum number of cycles allowed by the
                // duration constraint (and the error rate).
                let num_cycles = num_cycles_required_for_magic_states
                    .max(num_cycles_required_by_layout_overhead);

                if let Some(max_factories) = self.max_factories
                    && num_factories > max_factories
                {
                    continue;
                }

                let result = PhysicalResourceEstimationResult::new(
                    self,
                    LogicalPatch::new(&self.ftp, code_parameter.clone(), self.qubit.clone())?,
                    error_budget,
                    num_cycles,
                    vec![Some(FactoryPart::new(
                        factory.into_owned(),
                        num_factories,
                        num_magic_states,
                        required_logical_magic_state_error_rate,
                    ))],
                    required_logical_error_rate,
                )?;

                if best_estimation_result
                    .as_ref()
                    .is_none_or(|r| result.physical_qubits() < r.physical_qubits())
                {
                    best_estimation_result = Some(result);
                }
            }
        }

        best_estimation_result.ok_or(Error::MaxDurationTooSmall)
    }

    #[allow(clippy::too_many_lines)]
    pub fn estimate_with_max_num_qubits(
        &self,
        error_budget: &ErrorBudget,
        max_num_qubits: u64,
    ) -> Result<PhysicalResourceEstimationResult<E, Builder::Factory>, Error> {
        if self.factory_builder.num_magic_state_types() != 1 {
            return Err(Error::MultipleMagicStatesNotSupported);
        }

        let InitialOptimizationValues {
            min_code_parameter,
            num_cycles_required_by_layout_overhead,
            required_logical_error_rate,
            required_logical_magic_state_error_rate,
        } = self.compute_initial_optimization_values(error_budget)?;

        let num_magic_states = self
            .layout_overhead
            .num_magic_states(error_budget, 0)
            .map_err(Error::NumberOfMagicStatesComputationFailed)?;
        if num_magic_states == 0 {
            let logical_patch =
                LogicalPatch::new(&self.ftp, min_code_parameter, self.qubit.clone())?;
            if self.num_algorithmic_physical_qubits(&logical_patch)? <= max_num_qubits {
                return PhysicalResourceEstimationResult::without_factories(
                    self,
                    logical_patch,
                    error_budget,
                    num_cycles_required_by_layout_overhead,
                    required_logical_error_rate,
                );
            }
            return Err(Error::MaxPhysicalQubitsTooSmall);
        }

        let mut best_estimation_result: Option<
            PhysicalResourceEstimationResult<E, Builder::Factory>,
        > = None;

        let mut last_factories = Vec::new();
        let mut last_code_parameter = None;

        for code_parameter in self
            .ftp
            .code_parameter_range(Some(&min_code_parameter))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            let logical_patch =
                LogicalPatch::new(&self.ftp, code_parameter.clone(), self.qubit.clone())?;

            let physical_qubits_for_algorithm =
                self.num_algorithmic_physical_qubits(&logical_patch)?;
            if max_num_qubits <= physical_qubits_for_algorithm {
                continue;
            }
            let physical_qubits_allowed_for_magic_states =
                max_num_qubits - physical_qubits_for_algorithm;

            let max_num_cycles_allowed_by_error_rate =
                self.logical_cycles_for_code_parameter(error_budget.logical(), &code_parameter)?;

            if max_num_cycles_allowed_by_error_rate < num_cycles_required_by_layout_overhead {
                continue;
            }

            // The initial value for the last code parameter is `None`. This
            // ensures that the first code parameter is always tried. After
            // that, the last code parameter governs the reuse of the magic
            // state factory.
            if last_code_parameter.as_ref().is_none_or(|d| {
                self.ftp
                    .code_parameter_cmp(self.qubit.as_ref(), d, &code_parameter)
                    .is_gt()
            }) {
                last_factories = self
                    .factory_builder
                    .find_factories(
                        &self.ftp,
                        &self.qubit,
                        0,
                        required_logical_magic_state_error_rate,
                        &code_parameter,
                    )
                    .map_err(Error::FactorySearchFailed)?;

                last_code_parameter = self.find_highest_code_parameter(&last_factories);
            }

            if let Some(factory) = Self::try_pick_factory_below_or_equal_num_qubits(
                &last_factories,
                physical_qubits_allowed_for_magic_states,
            ) {
                // need only integer part of num_factories
                let num_factories =
                    physical_qubits_allowed_for_magic_states / factory.physical_qubits();

                if num_factories == 0 {
                    continue;
                }

                let num_cycles_required_for_magic_states = self
                    .compute_num_cycles_required_for_magic_states(
                        0,
                        num_factories,
                        &factory,
                        &logical_patch,
                        error_budget,
                    )?;

                let num_cycles = num_cycles_required_for_magic_states
                    .max(num_cycles_required_by_layout_overhead);

                if num_cycles > max_num_cycles_allowed_by_error_rate {
                    continue;
                }

                if let Some(max_factories) = self.max_factories
                    && num_factories > max_factories
                {
                    continue;
                }

                let result = PhysicalResourceEstimationResult::new(
                    self,
                    logical_patch,
                    error_budget,
                    num_cycles,
                    vec![Some(FactoryPart::new(
                        factory.into_owned(),
                        num_factories,
                        num_magic_states,
                        required_logical_magic_state_error_rate,
                    ))],
                    required_logical_error_rate,
                )?;

                if best_estimation_result
                    .as_ref()
                    .is_none_or(|r| result.runtime() < r.runtime())
                {
                    best_estimation_result = Some(result);
                }
            }
        }

        best_estimation_result.ok_or(Error::MaxPhysicalQubitsTooSmall)
    }

    /// Based on `num_factories`, we compute the number of cycles required which
    /// must be smaller than the maximum number of cycles allowed by the
    /// duration constraint (and the error rate).
    fn compute_num_cycles_required_for_magic_states(
        &self,
        magic_state_index: usize,
        num_factories: u64,
        factory: &Builder::Factory,
        logical_patch: &LogicalPatch<E>,
        error_budget: &ErrorBudget,
    ) -> Result<u64, Error> {
        let magic_states_per_run = num_factories * factory.num_output_states();

        let required_runs = self
            .layout_overhead
            .num_magic_states(error_budget, magic_state_index)
            .map_err(Error::NumberOfMagicStatesComputationFailed)?
            .div_ceil(magic_states_per_run);

        let required_duration = required_runs * factory.duration();
        Ok(required_duration.div_ceil(logical_patch.logical_cycle_time()))
    }

    fn try_pick_factory_below_or_equal_num_qubits<'a>(
        factories: &[Cow<'a, Builder::Factory>],
        max_num_qubits: u64,
    ) -> Option<Cow<'a, Builder::Factory>> {
        factories
            .iter()
            .filter(|p| p.physical_qubits() <= max_num_qubits)
            .min_by(|&p, &q| {
                p.normalized_volume()
                    .partial_cmp(&q.normalized_volume())
                    .expect("Could not compare factories normalized volume")
            })
            .cloned()
    }

    fn pick_factories_with_num_cycles<'a, 'b>(
        factories: &'b [Cow<'a, Builder::Factory>],
        logical_patch: &'b LogicalPatch<E>,
        max_cycles: u64,
    ) -> impl Iterator<Item = FactoryForCycles<'a, Builder::Factory>> + 'b {
        factories.iter().filter_map(move |factory| {
            let num = factory
                .duration()
                .div_ceil(logical_patch.logical_cycle_time());
            (num <= max_cycles).then_some(FactoryForCycles::new(factory.clone(), num))
        })
    }

    fn find_highest_code_parameter(
        &self,
        factories: &[Cow<Builder::Factory>],
    ) -> Option<E::Parameter> {
        factories
            .iter()
            .filter_map(|f| f.as_ref().max_code_parameter())
            .max_by(|a, b| self.ftp.code_parameter_cmp(self.qubit.as_ref(), a, b))
            .map(Cow::into_owned)
    }

    /// Computes the number of algorithmic physical qubits given the layout
    /// overhead and a logical patch
    fn num_algorithmic_physical_qubits(&self, patch: &LogicalPatch<E>) -> Result<u64, Error> {
        // the number of logical patches required for the algorithm given
        // a logical patch
        let num_logical_patches = self
            .layout_overhead
            .logical_qubits()
            .map_err(Error::AlgorithmicLogicalQubitsComputationFailed)?
            .div_ceil(patch.logical_qubits());

        Ok(num_logical_patches * patch.physical_qubits())
    }

    fn volume(&self, num_cycles: u64) -> Result<u64, Error> {
        Ok(self
            .layout_overhead
            .logical_qubits()
            .map_err(Error::AlgorithmicLogicalQubitsComputationFailed)?
            * num_cycles)
    }

    /// Computes required logical error rate for a logical operation one one
    /// qubit
    ///
    /// The logical volume is the number of logical qubits times the number of
    /// cycles.  We obtain the required logical error rate by dividing the error
    /// budget for logical operations by the volume.
    fn required_logical_error_rate(
        &self,
        logical_error_budget: f64,
        num_cycles: u64,
    ) -> Result<f64, Error> {
        Ok(logical_error_budget / self.volume(num_cycles)? as f64)
    }

    /// Computes the code parameter for the required logical error rate
    fn compute_code_parameter(&self, error_rate: f64) -> Result<E::Parameter, Error> {
        let parameter = self
            .ftp
            .compute_code_parameter(&self.qubit, error_rate)
            .map_err(Error::CodeParameterComputationFailed)?;

        self.ftp
            .adjust_code_parameter(parameter)
            .map_err(Error::CodeParameterComputationFailed)
    }

    /// Computes the number of possible cycles given a chosen code parameter
    fn logical_cycles_for_code_parameter(
        &self,
        logical_error_budget: f64,
        code_parameter: &E::Parameter,
    ) -> Result<u64, Error> {
        // Compute the achievable error rate for the code parameter
        let error_rate = self
            .ftp
            .logical_error_rate(&self.qubit, code_parameter)
            .map_err(Error::LogicalErrorRateComputationFailed)?;

        Ok((logical_error_budget
            / (self
                .layout_overhead
                .logical_qubits()
                .map_err(Error::AlgorithmicLogicalQubitsComputationFailed)? as f64
                * error_rate))
            .floor() as u64)
    }

    // Possibly adjusts number of cycles C from initial starting point C_min
    fn compute_num_cycles(&self, error_budget: &ErrorBudget) -> Result<u64, Error> {
        // Start loop with C = C_min
        let mut num_cycles = self
            .layout_overhead
            .logical_depth(error_budget)
            .map_err(Error::AlgorithmicLogicalDepthComputationFailed)?;

        // Perform logical depth scaling if given by constraint
        if let Some(logical_depth_scaling) = self.logical_depth_factor {
            if logical_depth_scaling < 1.0 {
                return Err(Error::LogicalDepthScalingFactorTooSmall(
                    logical_depth_scaling,
                ));
            }

            num_cycles = ((num_cycles as f64) * logical_depth_scaling).ceil() as u64;
        }

        // We cannot perform resource estimation when there are neither magic states nor cycles
        if num_cycles == 0 {
            for index in 0..self.factory_builder.num_magic_state_types() {
                if self
                    .layout_overhead
                    .num_magic_states(error_budget, index)
                    .map_err(Error::NumberOfMagicStatesComputationFailed)?
                    > 0
                {
                    return Ok(num_cycles);
                }
            }
            return Err(Error::AlgorithmHasNoResources);
        }

        Ok(num_cycles)
    }

    // Choose number of factories to use; we can safely use unwrap on the number
    // of magic states here, because the algorithm only finds factories that
    // provide this number
    fn num_factories(
        &self,
        logical_patch: &LogicalPatch<E>,
        magic_state_index: usize,
        factory: &Builder::Factory,
        error_budget: &ErrorBudget,
        num_cycles: u64,
    ) -> Result<u64, Error> {
        // first, try with the exact calculation; if that does not work, use
        // floating-point arithmetic, which may cause numeric imprecision
        if let Some(total_duration) = num_cycles.checked_mul(logical_patch.logical_cycle_time()) {
            // number of magic states that one factory can compute in num_cycles
            let num_states_per_run =
                (total_duration / factory.duration()) * factory.num_output_states();
            Ok(self
                .layout_overhead
                .num_magic_states(error_budget, magic_state_index)
                .map_err(Error::NumberOfMagicStatesComputationFailed)?
                .div_ceil(num_states_per_run))
        } else {
            let magic_states_per_cycles =
                self.layout_overhead
                    .num_magic_states(error_budget, magic_state_index)
                    .map_err(Error::NumberOfMagicStatesComputationFailed)? as f64
                    / (factory.num_output_states() * num_cycles) as f64;

            let factory_duration_fraction =
                factory.duration() as f64 / logical_patch.logical_cycle_time() as f64;

            Ok((magic_states_per_cycles * factory_duration_fraction).ceil() as _)
        }
    }
}

struct InitialOptimizationValues<Parameter> {
    min_code_parameter: Parameter,
    num_cycles_required_by_layout_overhead: u64,
    required_logical_error_rate: f64,
    required_logical_magic_state_error_rate: f64,
}

/// Models a factory that can be used, if one assumes a specific number of
/// cycles to run the algorithm
struct FactoryForCycles<'a, F: Clone> {
    factory: Cow<'a, F>,
    num_cycles: u64,
}

impl<'a, F: Clone> FactoryForCycles<'a, F> {
    pub fn new(factory: Cow<'a, F>, num_cycles: u64) -> Self {
        Self {
            factory,
            num_cycles,
        }
    }
}

impl<F: Factory + Clone> Ord for FactoryForCycles<'_, F> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.factory
            .normalized_volume()
            .total_cmp(&other.factory.normalized_volume())
            .then_with(|| self.num_cycles.cmp(&other.num_cycles))
    }
}

impl<F: Factory + Clone> PartialOrd for FactoryForCycles<'_, F> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<F: Factory + Clone> PartialEq for FactoryForCycles<'_, F> {
    fn eq(&self, other: &Self) -> bool {
        (self.factory.normalized_volume(), self.num_cycles)
            == (other.factory.normalized_volume(), other.num_cycles)
    }
}

impl<F: Factory + Clone> Eq for FactoryForCycles<'_, F> {}
