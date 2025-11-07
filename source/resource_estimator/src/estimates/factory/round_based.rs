use probability::{distribution::Inverse, prelude::Binomial};
use std::borrow::Cow;

use super::Factory;

pub trait DistillationUnit<P> {
    fn num_output_states(&self) -> u64;
    fn num_input_states(&self) -> u64;
    fn duration(&self, position: usize) -> u64;
    fn physical_qubits(&self, position: usize) -> u64;
    fn name(&self) -> &str;
    fn code_parameter(&self) -> Option<&P>;
    fn output_error_rate(&self, input_error_rate: f64) -> f64;
    fn failure_probability(&self, input_error_rate: f64) -> f64;
}

#[derive(Debug)]
pub enum FactoryBuildError {
    LowFailureProbability,
    HighFailureProbability,
    OutputErrorRateHigherThanInputErrorRate,
    UnreasonableHighNumberOfUnitsRequired,
}

/// One round of distillation in a factory
///
/// All units per round are the same.  The initial number of units is 1 and can
/// be iteratively adjusted to match some external constraints.
#[derive(Debug, Clone)]
pub struct DistillationRound<P> {
    num_units: u64,
    failure_probability_requirement: f64,
    num_output_states: u64,
    num_input_states: u64,
    duration: u64,
    physical_qubits: u64,
    name: String,
    code_parameter: Option<P>,
}

impl<P: Clone> DistillationRound<P> {
    pub fn new(
        unit: &impl DistillationUnit<P>,
        failure_probability_requirement: f64,
        position: usize,
    ) -> Self {
        Self {
            num_units: 1,
            failure_probability_requirement,
            num_output_states: unit.num_output_states(),
            num_input_states: unit.num_input_states(),
            duration: unit.duration(position),
            physical_qubits: unit.physical_qubits(position),
            name: unit.name().into(),
            code_parameter: unit.code_parameter().cloned(),
        }
    }

    pub fn adjust_num_units_to(
        &mut self,
        output_states_needed_next: u64,
        failure_probability: f64,
    ) -> Result<(), FactoryBuildError> {
        // initial value
        self.num_units = ((output_states_needed_next as f64)
            / (self.max_num_output_states() as f64))
            .ceil() as u64;

        loop {
            let num_output_states = self.compute_num_output_states(failure_probability);
            if num_output_states < output_states_needed_next {
                self.num_units *= 2;

                // TFactory distillation round requires unreasonably high number of units?
                if self.num_units >= 1_000_000_000_000_000 {
                    return Err(FactoryBuildError::UnreasonableHighNumberOfUnitsRequired);
                }
            } else {
                break;
            }
        }

        let mut upper = self.num_units;
        let mut lower = self.num_units / 2;
        while lower < upper {
            self.num_units = u64::midpoint(lower, upper);
            let num_output_ts = self.compute_num_output_states(failure_probability);
            if num_output_ts >= output_states_needed_next {
                upper = self.num_units;
            } else {
                lower = self.num_units + 1;
            }
        }
        self.num_units = upper;

        Ok(())
    }

    pub fn physical_qubits(&self) -> u64 {
        self.num_units * self.physical_qubits
    }

    pub fn duration(&self) -> u64 {
        self.duration
    }

    pub fn compute_num_output_states(&self, failure_probability: f64) -> u64 {
        // special case when not necessary to run actual distillation:
        // the physical qubit error rate is already below the threshold
        if failure_probability == 0.0 && self.failure_probability_requirement == 0.0 {
            return self.num_units * self.num_output_states;
        }
        let dist = Binomial::with_failure(self.num_units as usize, failure_probability);
        dist.inverse(self.failure_probability_requirement) as u64 * self.num_output_states
    }

    fn max_num_output_states(&self) -> u64 {
        self.num_units * self.num_output_states
    }

    pub fn num_units(&self) -> u64 {
        self.num_units
    }
}

#[derive(Debug, Clone)]
pub struct RoundBasedFactory<P> {
    length: usize,
    failure_probability_requirement: f64,
    rounds: Vec<DistillationRound<P>>,
    input_error_rate_before_each_round: Vec<f64>,
    failure_probability_after_each_round: Vec<f64>,
    physical_qubit_calculation: PhysicalQubitCalculation,
}

impl<P: Clone> RoundBasedFactory<P> {
    #[must_use]
    pub fn new(
        length: usize,
        failure_probability_requirement: f64,
        rounds: Vec<DistillationRound<P>>,
        input_error_rate_before_each_round: Vec<f64>,
        failure_probability_after_each_round: Vec<f64>,
    ) -> Self {
        Self {
            length,
            failure_probability_requirement,
            rounds,
            input_error_rate_before_each_round,
            failure_probability_after_each_round,
            physical_qubit_calculation: PhysicalQubitCalculation::default(),
        }
    }

    pub fn build(
        units: &[&impl DistillationUnit<P>],
        initial_input_error_rate: f64,
        failure_probability_requirement: f64,
    ) -> Result<RoundBasedFactory<P>, FactoryBuildError> {
        let rounds: Vec<DistillationRound<P>> = Vec::with_capacity(units.len());
        let mut input_error_rate_before_each_round = Vec::with_capacity(units.len() + 1);
        input_error_rate_before_each_round.push(initial_input_error_rate);
        let failure_probability_after_each_round: Vec<f64> = vec![1.0; units.len() + 1];

        let mut pipeline = Self {
            length: units.len(),
            failure_probability_requirement,
            rounds,
            input_error_rate_before_each_round,
            failure_probability_after_each_round,
            physical_qubit_calculation: PhysicalQubitCalculation::default(),
        };

        pipeline.compute_units_per_round(units, 1)?;

        Ok(pipeline)
    }

    fn add_rounds(&mut self, units: &[&impl DistillationUnit<P>]) -> Result<(), FactoryBuildError> {
        for unit in units {
            let failure_probability_requirement =
                self.failure_probability_requirement / (self.length as f64);
            let &input_error_rate = self
                .input_error_rate_before_each_round
                .last()
                .unwrap_or_else(|| unreachable!());
            let output_error_rate = unit.output_error_rate(input_error_rate);
            if output_error_rate > input_error_rate {
                return Err(FactoryBuildError::OutputErrorRateHigherThanInputErrorRate);
            }
            let round =
                DistillationRound::new(*unit, failure_probability_requirement, self.rounds.len());
            self.rounds.push(round);
            self.input_error_rate_before_each_round
                .push(output_error_rate);
        }

        Ok(())
    }

    #[must_use]
    pub fn physical_qubit_calculation(&self) -> PhysicalQubitCalculation {
        self.physical_qubit_calculation
    }

    pub fn set_physical_qubit_calculation(
        &mut self,
        physical_qubit_calculation: PhysicalQubitCalculation,
    ) {
        self.physical_qubit_calculation = physical_qubit_calculation;
    }

    #[must_use]
    pub fn rounds(&self) -> &[DistillationRound<P>] {
        &self.rounds
    }

    /// Number of distillation rounds
    #[must_use]
    pub fn num_rounds(&self) -> u64 {
        self.length as u64
    }

    /// Number of units per distillation round
    #[must_use]
    pub fn num_units_per_round(&self) -> Vec<u64> {
        self.rounds.iter().map(|round| round.num_units).collect()
    }

    /// Physical qubits per round
    pub fn physical_qubits_per_round(&self) -> Vec<u64> {
        self.rounds
            .iter()
            .map(DistillationRound::physical_qubits)
            .collect()
    }

    /// Runtime in ns per round
    pub fn duration_per_round(&self) -> Vec<u64> {
        self.rounds
            .iter()
            .map(DistillationRound::duration)
            .collect()
    }

    /// Names of distillation units per round
    #[must_use]
    pub fn unit_names(&self) -> Vec<String> {
        self.rounds.iter().map(|round| round.name.clone()).collect()
    }

    /// This computes the necessary number of units per round in order to
    /// achieve the required success probability
    /// Returning None means that the sequence of units does not provide a TFactory with the required output error rate.
    #[allow(clippy::doc_markdown)]
    pub fn compute_units_per_round(
        &mut self,
        units: &[&impl DistillationUnit<P>],
        multiplier: u64,
    ) -> Result<(), FactoryBuildError> {
        self.add_rounds(units)?;

        if self.length > 0 {
            let mut states_needed_next =
                self.rounds[self.length - 1].num_output_states * multiplier;

            for idx in (0..self.length).rev() {
                let q =
                    units[idx].failure_probability(self.input_error_rate_before_each_round[idx]);
                if q <= 0.0 {
                    return Err(FactoryBuildError::LowFailureProbability);
                }

                if q >= 1.0 {
                    return Err(FactoryBuildError::HighFailureProbability);
                }

                self.failure_probability_after_each_round[idx] = q;
                self.rounds[idx].adjust_num_units_to(states_needed_next, q)?;

                states_needed_next =
                    self.rounds[idx].num_input_states * self.rounds[idx].num_units();
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn input_error_rate(&self) -> f64 {
        // Even when there are no units `input_error_rate_before_each_round`
        // has one element
        self.input_error_rate_before_each_round[0]
    }

    #[must_use]
    pub fn output_error_rate(&self) -> f64 {
        self.input_error_rate_before_each_round[self.length]
    }

    #[must_use]
    pub fn num_input_states(&self) -> u64 {
        self.rounds
            .first()
            .map_or(0, |round| round.num_input_states * round.num_units())
    }

    #[must_use]
    pub fn normalized_qubits(&self) -> f64 {
        (self.physical_qubits() as f64) / (self.num_output_states() as f64)
    }

    /// Code parameter per round
    #[must_use]
    pub fn code_parameter_per_round(&self) -> Vec<Option<&P>> {
        self.rounds
            .iter()
            .map(|round| round.code_parameter.as_ref())
            .collect()
    }
}

impl<P: Clone> Factory for RoundBasedFactory<P> {
    type Parameter = P;

    fn physical_qubits(&self) -> u64 {
        match self.physical_qubit_calculation {
            PhysicalQubitCalculation::Max => self
                .rounds
                .iter()
                .map(DistillationRound::physical_qubits)
                .max()
                .unwrap_or(0),
            PhysicalQubitCalculation::Sum => self
                .rounds
                .iter()
                .map(DistillationRound::physical_qubits)
                .sum::<u64>(),
        }
    }

    fn duration(&self) -> u64 {
        self.rounds.iter().map(DistillationRound::duration).sum()
    }

    fn num_output_states(&self) -> u64 {
        let last_round = self
            .rounds
            .last()
            .expect("at least one round should be present");
        let failure_probability = self.failure_probability_after_each_round[self.length - 1];
        // This should not fail, as we already evaluated this
        // failure_probability when building the factory
        last_round.compute_num_output_states(failure_probability)
    }

    fn max_code_parameter(&self) -> Option<Cow<'_, P>> {
        self.code_parameter_per_round()
            .last()
            .expect("at least one round should be present")
            .map(|f| Cow::Borrowed(f))
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub enum PhysicalQubitCalculation {
    /// physical qubits can be shared among rounds
    #[default]
    Max,
    /// each round has its own physical qubits
    Sum,
}
