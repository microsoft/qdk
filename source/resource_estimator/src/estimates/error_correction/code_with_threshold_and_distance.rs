use super::ErrorCorrection;

pub trait CodeWithThresholdAndDistanceEvaluator {
    type Qubit;

    fn physical_error_rate(&self, qubit: &Self::Qubit) -> f64;
    fn physical_qubits(&self, code_distance: u64) -> Result<u64, String>;
    fn logical_cycle_time(&self, qubit: &Self::Qubit, code_distance: u64) -> Result<u64, String>;
}

pub struct CodeWithThresholdAndDistance<Evaluator> {
    evaluator: Evaluator,
    crossing_prefactor: f64,
    error_correction_threshold: f64,
    distance_coefficient_power: u32,
    max_code_distance: Option<u64>,
}

impl<Evaluator: CodeWithThresholdAndDistanceEvaluator> CodeWithThresholdAndDistance<Evaluator> {
    pub fn new(
        evaluator: Evaluator,
        crossing_prefactor: f64,
        error_correction_threshold: f64,
        distance_coefficient_power: u32,
    ) -> Self {
        Self {
            evaluator,
            crossing_prefactor,
            error_correction_threshold,
            distance_coefficient_power,
            max_code_distance: None,
        }
    }

    pub fn with_max_code_distance(
        evaluator: Evaluator,
        crossing_prefactor: f64,
        error_correction_threshold: f64,
        distance_coefficient_power: u32,
        max_code_distance: u64,
    ) -> Self {
        Self {
            evaluator,
            crossing_prefactor,
            error_correction_threshold,
            distance_coefficient_power,
            max_code_distance: Some(max_code_distance),
        }
    }

    pub fn crossing_prefactor(&self) -> f64 {
        self.crossing_prefactor
    }

    pub fn set_crossing_prefactor(&mut self, crossing_prefactor: f64) {
        self.crossing_prefactor = crossing_prefactor;
    }

    pub fn error_correction_threshold(&self) -> f64 {
        self.error_correction_threshold
    }

    pub fn set_error_correction_threshold(&mut self, error_correction_threshold: f64) {
        self.error_correction_threshold = error_correction_threshold;
    }

    pub fn distance_coefficient_power(&self) -> u32 {
        self.distance_coefficient_power
    }

    pub fn set_distance_coefficient_power(&mut self, distance_coefficient_power: u32) {
        self.distance_coefficient_power = distance_coefficient_power;
    }

    pub fn max_code_distance(&self) -> Option<&u64> {
        self.max_code_distance.as_ref()
    }

    pub fn set_max_code_distance(&mut self, max_code_distance: u64) {
        self.max_code_distance = Some(max_code_distance);
    }

    pub fn evaluator(&self) -> &Evaluator {
        &self.evaluator
    }

    pub fn evaluator_mut(&mut self) -> &mut Evaluator {
        &mut self.evaluator
    }

    /// When the distance coefficient power is 0, the function can be easily
    /// inverted and we can avoid looping over all code parameters
    fn compute_smallest_code_parameter_with_inverse_formula(
        &self,
        qubit: &Evaluator::Qubit,
        required_logical_error_rate: f64,
    ) -> Result<u64, String> {
        // Compute code distance d (Equation (E2) in paper)
        let physical_error_rate = self.evaluator.physical_error_rate(qubit);
        let numerator = 2.0 * (self.crossing_prefactor / required_logical_error_rate).ln();
        let denominator = (self.error_correction_threshold / physical_error_rate).ln();

        let code_distance = (((numerator / denominator) - 1.0).ceil() as u64) | 0x1;

        if let Some(max_distance) = self.max_code_distance
            && max_distance < code_distance
        {
            return Err(format!(
                "The computed code distance {code_distance} is too high; maximum allowed code distance is {max_distance}; try increasing the total logical error budget"
            ));
        }

        Ok(code_distance)
    }
}

impl<Evaluator: CodeWithThresholdAndDistanceEvaluator> ErrorCorrection
    for CodeWithThresholdAndDistance<Evaluator>
{
    type Qubit = Evaluator::Qubit;
    type Parameter = u64;

    fn physical_qubits(&self, code_distance: &u64) -> Result<u64, String> {
        self.evaluator.physical_qubits(*code_distance)
    }

    fn logical_qubits(&self, _code_distance: &u64) -> Result<u64, String> {
        Ok(1)
    }

    fn logical_cycle_time(&self, qubit: &Self::Qubit, code_distance: &u64) -> Result<u64, String> {
        self.evaluator.logical_cycle_time(qubit, *code_distance)
    }

    fn logical_error_rate(&self, qubit: &Self::Qubit, code_distance: &u64) -> Result<f64, String> {
        let physical_error_rate = self.evaluator.physical_error_rate(qubit);

        if physical_error_rate > self.error_correction_threshold {
            Err(format!(
                "invalid value for 'physical_error_rate', expected value between 0 and {}",
                self.error_correction_threshold
            ))
        } else {
            Ok(self.crossing_prefactor
                * (code_distance.pow(self.distance_coefficient_power) as f64)
                * ((physical_error_rate / self.error_correction_threshold)
                    .powi((*code_distance as i32 + 1) / 2)))
        }
    }

    fn compute_code_parameter(
        &self,
        qubit: &Self::Qubit,
        required_logical_qubit_error_rate: f64,
    ) -> Result<u64, String> {
        if self.distance_coefficient_power == 0 {
            self.compute_smallest_code_parameter_with_inverse_formula(
                qubit,
                required_logical_qubit_error_rate,
            )
        } else {
            self.compute_smallest_code_parameter(qubit, required_logical_qubit_error_rate)
        }
    }

    fn code_parameter_range(
        &self,
        lower_bound: Option<&Self::Parameter>,
    ) -> impl Iterator<Item = Self::Parameter> {
        (lower_bound.copied().unwrap_or(1)..=self.max_code_distance.unwrap_or(u64::MAX)).step_by(2)
    }

    fn code_parameter_cmp(
        &self,
        _qubit: &Self::Qubit,
        p1: &Self::Parameter,
        p2: &Self::Parameter,
    ) -> std::cmp::Ordering {
        p1.cmp(p2)
    }
}
