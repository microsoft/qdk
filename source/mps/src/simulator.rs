// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{time::Duration, time::Instant};

use crate::rng::MeasurementRng;
use crate::{
    CapStatus, CapabilityStatus, EngineDescriptor, ExecutionPolicy, ExecutionReport, Measurement,
    MpsCapabilities, MpsEngine, MpsError, Operation, OperationCounts, OperationOutcome,
    PauliObservable, QubitId, ReleaseOutcome, SiteId, SitePauli, TimingReport,
};
use rustc_hash::FxHashSet;

const PROBABILITY_TOLERANCE: f64 = 1.0e-10;

pub struct MpsSimulator<E: MpsEngine> {
    policy: ExecutionPolicy,
    engine: E,
    logical_to_site: Vec<Option<SiteId>>,
    free_sites: Vec<SiteId>,
    rng: MeasurementRng,
    descriptor: EngineDescriptor,
    capabilities: MpsCapabilities,
    operation_counts: OperationCounts,
    timings: TimingReport,
    norm_before_first_non_unitary: Option<f64>,
}

impl<E: MpsEngine> MpsSimulator<E> {
    pub(crate) fn from_resolved(
        policy: ExecutionPolicy,
        engine: E,
        descriptor: EngineDescriptor,
        capabilities: MpsCapabilities,
        initialization: Duration,
    ) -> Result<Self, MpsError> {
        policy.validate()?;
        Ok(Self {
            rng: MeasurementRng::new(policy.shot_seed),
            policy,
            engine,
            logical_to_site: Vec::new(),
            free_sites: Vec::new(),
            descriptor,
            capabilities,
            operation_counts: OperationCounts::default(),
            timings: TimingReport {
                initialization,
                ..TimingReport::default()
            },
            norm_before_first_non_unitary: None,
        })
    }

    pub fn allocate(&mut self) -> Result<QubitId, MpsError> {
        let site = if let Some(site) = self.free_sites.pop() {
            site
        } else {
            self.engine.append_zero_site()?
        };
        if let Some((index, slot)) = self
            .logical_to_site
            .iter_mut()
            .enumerate()
            .find(|(_, site)| site.is_none())
        {
            *slot = Some(site);
            return Ok(QubitId(u64::try_from(index).map_err(|_| {
                MpsError::InternalInvariant("logical qubit ID exceeds u64".into())
            })?));
        }
        let id = QubitId(
            u64::try_from(self.logical_to_site.len())
                .map_err(|_| MpsError::InternalInvariant("logical qubit ID exceeds u64".into()))?,
        );
        self.logical_to_site.push(Some(site));
        Ok(id)
    }

    pub fn release(&mut self, qubit: QubitId) -> Result<ReleaseOutcome, MpsError> {
        let site = self.site(qubit)?;
        self.capture_pre_non_unitary_norm()?;
        let start = Instant::now();
        let probability_one = valid_probability(self.engine.probability_one(site)?)?;
        let was_zero = probability_one <= PROBABILITY_TOLERANCE;
        let outcome = self.sample_and_project(site, probability_one)?;
        if outcome == Measurement::One {
            self.engine
                .apply_one(site, &crate::OneQubitGate::X.matrix())?;
        }
        self.timings.measurement_reset += start.elapsed();
        self.operation_counts.measurement += 1;
        self.operation_counts.reset += 1;
        let index = Self::logical_index(qubit)?;
        self.logical_to_site[index] = None;
        self.free_sites.push(site);
        Ok(ReleaseOutcome { was_zero })
    }

    pub fn swap_ids(&mut self, first: QubitId, second: QubitId) -> Result<(), MpsError> {
        if first == second {
            return Err(MpsError::DuplicateQubit(first));
        }
        self.site(first)?;
        self.site(second)?;
        let first_index = Self::logical_index(first)?;
        let second_index = Self::logical_index(second)?;
        self.logical_to_site.swap(first_index, second_index);
        Ok(())
    }

    pub fn apply(&mut self, operation: Operation) -> Result<OperationOutcome, MpsError> {
        match operation {
            Operation::One { gate, target } => {
                let site = self.site(target)?;
                let start = Instant::now();
                self.engine.apply_one(site, &gate.matrix())?;
                self.timings.unitary += start.elapsed();
                self.operation_counts.one_qubit += 1;
                Ok(OperationOutcome::Unit)
            }
            Operation::Two {
                gate,
                first,
                second,
            } => {
                if first == second {
                    return Err(MpsError::DuplicateQubit(first));
                }
                let first_site = self.site(first)?;
                let second_site = self.site(second)?;
                if first_site.0.abs_diff(second_site.0) != 1 {
                    return Err(MpsError::CapabilityNotImplemented(
                        "non-local two-qubit routing".into(),
                    ));
                }
                let start = Instant::now();
                self.engine
                    .apply_adjacent_two(first_site, second_site, &gate.matrix())?;
                self.timings.unitary += start.elapsed();
                self.operation_counts.two_qubit += 1;
                Ok(OperationOutcome::Unit)
            }
            Operation::MeasureZ { target } => {
                let measurement = self.measure(target)?;
                Ok(OperationOutcome::Measurement(measurement))
            }
            Operation::MeasureResetZ { target } => {
                let measurement = self.measure(target)?;
                if measurement == Measurement::One {
                    let start = Instant::now();
                    let site = self.site(target)?;
                    self.engine
                        .apply_one(site, &crate::OneQubitGate::X.matrix())?;
                    self.timings.measurement_reset += start.elapsed();
                    self.operation_counts.reset += 1;
                }
                Ok(OperationOutcome::Measurement(measurement))
            }
            Operation::ResetZ { target } => {
                let measurement = self.measure(target)?;
                if measurement == Measurement::One {
                    let site = self.site(target)?;
                    self.engine
                        .apply_one(site, &crate::OneQubitGate::X.matrix())?;
                }
                self.operation_counts.reset += 1;
                Ok(OperationOutcome::Unit)
            }
        }
    }

    pub fn expectation(&mut self, observable: &PauliObservable) -> Result<f64, MpsError> {
        let start = Instant::now();
        let norm = self.engine.state_norm()?;
        if norm <= PROBABILITY_TOLERANCE {
            return Err(MpsError::InternalInvariant(
                "cannot evaluate an observable on a zero-norm state".into(),
            ));
        }
        let mut total = 0.0;
        for term in &observable.terms {
            if !term.coefficient.is_finite() {
                return Err(MpsError::InvalidPolicy(
                    "observable coefficients must be finite".into(),
                ));
            }
            let mut seen = FxHashSet::default();
            seen.reserve(term.factors.len());
            let factors = term
                .factors
                .iter()
                .map(|(qubit, pauli)| {
                    if !seen.insert(*qubit) {
                        return Err(MpsError::DuplicateQubit(*qubit));
                    }
                    Ok(SitePauli {
                        site: self.site(*qubit)?,
                        pauli: *pauli,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let value = self.engine.expectation_pauli_product(&factors)?;
            total += term.coefficient * value.re / norm.powi(2);
        }
        self.timings.observable += start.elapsed();
        self.operation_counts.observable += 1;
        Ok(total)
    }

    pub fn report(&mut self) -> Result<ExecutionReport, MpsError> {
        let state_norm = self.engine.state_norm()?;
        let reached_bond_dimension = self.engine.reached_bond_dimension();
        let cap_status =
            self.policy
                .truncation
                .max_bond_dimension
                .map_or(CapStatus::NotConfigured, |cap| {
                    if reached_bond_dimension >= cap.get() {
                        CapStatus::ReachedCapIndeterminate
                    } else {
                        CapStatus::BelowCap
                    }
                });
        let info = self.engine.info();
        if info.descriptor != self.descriptor {
            return Err(MpsError::InternalInvariant(
                "engine descriptor changed after factory resolution".into(),
            ));
        }
        Ok(ExecutionReport {
            requested_policy: self.policy.clone(),
            engine: info,
            capabilities: self.capabilities.clone(),
            resolved_seed: self.policy.shot_seed,
            operation_counts: self.operation_counts,
            timings: self.timings,
            state_norm,
            norm_before_first_non_unitary: self.norm_before_first_non_unitary,
            reached_bond_dimension,
            cap_status,
            local_threshold: self
                .policy
                .truncation
                .max_relative_discarded_squared_weight_per_split,
            discarded_weight: CapabilityStatus::Unavailable {
                reason: "the selected engine does not expose discarded weight".into(),
            },
        })
    }

    fn measure(&mut self, qubit: QubitId) -> Result<Measurement, MpsError> {
        let site = self.site(qubit)?;
        self.capture_pre_non_unitary_norm()?;
        let start = Instant::now();
        let probability_one = valid_probability(self.engine.probability_one(site)?)?;
        let result = self.sample_and_project(site, probability_one);
        self.timings.measurement_reset += start.elapsed();
        self.operation_counts.measurement += 1;
        result
    }

    fn sample_and_project(
        &mut self,
        site: SiteId,
        probability_one: f64,
    ) -> Result<Measurement, MpsError> {
        let outcome = if self.rng.next_f64() < probability_one {
            Measurement::One
        } else {
            Measurement::Zero
        };
        let selected_probability = match outcome {
            Measurement::Zero => 1.0 - probability_one,
            Measurement::One => probability_one,
        };
        if selected_probability <= 0.0 {
            return Err(MpsError::ZeroProbabilityProjection(outcome));
        }
        self.engine.project_z(site, outcome)?;
        Ok(outcome)
    }

    fn site(&self, qubit: QubitId) -> Result<SiteId, MpsError> {
        let index = Self::logical_index(qubit)?;
        self.logical_to_site
            .get(index)
            .copied()
            .flatten()
            .ok_or(MpsError::UnallocatedQubit(qubit))
    }

    fn logical_index(qubit: QubitId) -> Result<usize, MpsError> {
        usize::try_from(qubit.0).map_err(|_| MpsError::UnallocatedQubit(qubit))
    }

    fn capture_pre_non_unitary_norm(&mut self) -> Result<(), MpsError> {
        if self.norm_before_first_non_unitary.is_none() {
            self.norm_before_first_non_unitary = Some(self.engine.state_norm()?);
        }
        Ok(())
    }
}

fn valid_probability(probability: f64) -> Result<f64, MpsError> {
    if !probability.is_finite()
        || !(-PROBABILITY_TOLERANCE..=1.0 + PROBABILITY_TOLERANCE).contains(&probability)
    {
        return Err(MpsError::InvalidProbability(probability));
    }
    Ok(probability.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use num_complex::Complex64;

    use super::*;
    use crate::{
        Matrix2, Matrix4, MpsEngineFactory, OneQubitGate, Pauli, PauliTerm, Precision,
        ResourcePolicy, ResourceResolution, ResourceResolutionSource, TruncationPolicy,
        TwoQubitGate,
    };

    struct DenseEngine {
        state: Vec<Complex64>,
        info: crate::EngineInfo,
    }

    impl DenseEngine {
        fn new(descriptor: EngineDescriptor) -> Self {
            Self {
                state: vec![Complex64::ONE],
                info: crate::EngineInfo {
                    descriptor,
                    resources: ResourceResolution {
                        max_cpu_threads: NonZeroUsize::MIN,
                        source: ResourceResolutionSource::ProcessVisible,
                        caller_limit_honored: true,
                    },
                },
            }
        }

        fn qubit_count(&self) -> usize {
            self.state.len().ilog2() as usize
        }
    }

    impl MpsEngine for DenseEngine {
        fn info(&self) -> crate::EngineInfo {
            self.info.clone()
        }

        fn append_zero_site(&mut self) -> Result<SiteId, MpsError> {
            let site = SiteId(self.qubit_count());
            self.state.resize(self.state.len() * 2, Complex64::ZERO);
            Ok(site)
        }

        fn apply_one(&mut self, site: SiteId, matrix: &Matrix2) -> Result<(), MpsError> {
            let mask = 1 << site.0;
            for base in 0..self.state.len() {
                if base & mask == 0 {
                    let one = base | mask;
                    let zero_value = self.state[base];
                    let one_value = self.state[one];
                    self.state[base] = matrix[0][0] * zero_value + matrix[0][1] * one_value;
                    self.state[one] = matrix[1][0] * zero_value + matrix[1][1] * one_value;
                }
            }
            Ok(())
        }

        fn apply_adjacent_two(
            &mut self,
            first: SiteId,
            second: SiteId,
            matrix: &Matrix4,
        ) -> Result<(), MpsError> {
            if first.0.abs_diff(second.0) != 1 {
                return Err(MpsError::NonAdjacentOperands {
                    first: first.0,
                    second: second.0,
                });
            }
            let first_mask = 1 << first.0;
            let second_mask = 1 << second.0;
            for base in 0..self.state.len() {
                if base & (first_mask | second_mask) == 0 {
                    let indices = [
                        base,
                        base | first_mask,
                        base | second_mask,
                        base | first_mask | second_mask,
                    ];
                    let previous = indices.map(|index| self.state[index]);
                    for (row, index) in indices.into_iter().enumerate() {
                        self.state[index] = matrix[row]
                            .iter()
                            .zip(previous)
                            .map(|(entry, value)| entry * value)
                            .sum();
                    }
                }
            }
            Ok(())
        }

        fn probability_one(&mut self, site: SiteId) -> Result<f64, MpsError> {
            let mask = 1 << site.0;
            let norm_squared: f64 = self.state.iter().map(Complex64::norm_sqr).sum();
            Ok(self
                .state
                .iter()
                .enumerate()
                .filter(|(index, _)| index & mask != 0)
                .map(|(_, value)| value.norm_sqr())
                .sum::<f64>()
                / norm_squared)
        }

        fn project_z(&mut self, site: SiteId, outcome: Measurement) -> Result<(), MpsError> {
            let mask = 1 << site.0;
            for (index, value) in self.state.iter_mut().enumerate() {
                let is_one = index & mask != 0;
                if is_one != (outcome == Measurement::One) {
                    *value = Complex64::ZERO;
                }
            }
            let norm = self
                .state
                .iter()
                .map(Complex64::norm_sqr)
                .sum::<f64>()
                .sqrt();
            if norm == 0.0 {
                return Err(MpsError::ZeroProbabilityProjection(outcome));
            }
            for value in &mut self.state {
                *value /= norm;
            }
            Ok(())
        }

        fn expectation_pauli_product(&self, factors: &[SitePauli]) -> Result<Complex64, MpsError> {
            let mut transformed = self.state.clone();
            for factor in factors {
                let matrix = match factor.pauli {
                    Pauli::I => [
                        [Complex64::ONE, Complex64::ZERO],
                        [Complex64::ZERO, Complex64::ONE],
                    ],
                    Pauli::X => OneQubitGate::X.matrix(),
                    Pauli::Y => OneQubitGate::Y.matrix(),
                    Pauli::Z => OneQubitGate::Z.matrix(),
                };
                let mask = 1 << factor.site.0;
                for base in 0..transformed.len() {
                    if base & mask == 0 {
                        let one = base | mask;
                        let zero_value = transformed[base];
                        let one_value = transformed[one];
                        transformed[base] = matrix[0][0] * zero_value + matrix[0][1] * one_value;
                        transformed[one] = matrix[1][0] * zero_value + matrix[1][1] * one_value;
                    }
                }
            }
            Ok(self
                .state
                .iter()
                .zip(transformed)
                .map(|(left, right)| left.conj() * right)
                .sum())
        }

        fn state_norm(&mut self) -> Result<f64, MpsError> {
            Ok(self
                .state
                .iter()
                .map(Complex64::norm_sqr)
                .sum::<f64>()
                .sqrt())
        }

        fn reached_bond_dimension(&self) -> usize {
            1
        }
    }

    struct DenseFactory;

    impl DenseFactory {
        fn descriptor() -> EngineDescriptor {
            EngineDescriptor {
                name: "test-dense".into(),
                version: "0".into(),
                backend: "dense".into(),
                device: "cpu".into(),
            }
        }
    }

    impl MpsEngineFactory for DenseFactory {
        type Engine = DenseEngine;

        fn descriptor(&self) -> EngineDescriptor {
            Self::descriptor()
        }

        fn capabilities(&self) -> MpsCapabilities {
            let planned = || CapabilityStatus::Planned {
                reason: "not part of the test engine".into(),
            };
            MpsCapabilities {
                complex64: CapabilityStatus::Available,
                maximum_gate_arity: 2,
                dynamic_allocation: CapabilityStatus::Available,
                measurement_reset: CapabilityStatus::Available,
                non_local_routing: planned(),
                observables: CapabilityStatus::Available,
                noise: planned(),
                discarded_weight_diagnostics: planned(),
                constrained_cpu_resources: CapabilityStatus::Available,
                backend: "dense".into(),
                device: "cpu".into(),
            }
        }

        fn create_engine(&self, policy: &ExecutionPolicy) -> Result<Self::Engine, MpsError> {
            policy.validate()?;
            Ok(DenseEngine::new(Self::descriptor()))
        }
    }

    fn policy(seed: u64) -> ExecutionPolicy {
        ExecutionPolicy {
            precision: Precision::Complex64,
            truncation: TruncationPolicy {
                max_relative_discarded_squared_weight_per_split: Some(0.0),
                max_bond_dimension: None,
            },
            shot_seed: seed,
            resources: ResourcePolicy {
                max_cpu_threads: None,
            },
        }
    }

    #[test]
    fn bell_state_uses_little_endian_control_and_normalized_observable() {
        let mut simulator = DenseFactory.create_simulator(policy(7)).expect("simulator");
        let first = simulator.allocate().expect("first qubit");
        let second = simulator.allocate().expect("second qubit");
        simulator
            .apply(Operation::One {
                gate: OneQubitGate::H,
                target: first,
            })
            .expect("H");
        simulator
            .apply(Operation::Two {
                gate: TwoQubitGate::Cx,
                first,
                second,
            })
            .expect("CX");

        let zz = PauliObservable {
            terms: vec![PauliTerm {
                coefficient: 1.0,
                factors: vec![(first, Pauli::Z), (second, Pauli::Z)],
            }],
        };
        assert!((simulator.expectation(&zz).expect("ZZ") - 1.0).abs() < 1.0e-12);
        assert!((simulator.report().expect("report").state_norm - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn release_resets_and_reuses_lowest_logical_id() {
        let mut simulator = DenseFactory
            .create_simulator(policy(11))
            .expect("simulator");
        let first = simulator.allocate().expect("first qubit");
        simulator
            .apply(Operation::One {
                gate: OneQubitGate::X,
                target: first,
            })
            .expect("X");
        let outcome = simulator.release(first).expect("release");
        assert!(!outcome.was_zero);
        assert_eq!(simulator.allocate().expect("reallocate"), first);
        assert_eq!(
            simulator
                .apply(Operation::MeasureZ { target: first })
                .expect("measurement"),
            OperationOutcome::Measurement(Measurement::Zero)
        );
    }

    #[test]
    fn id_swap_does_not_apply_a_quantum_gate() {
        let mut simulator = DenseFactory
            .create_simulator(policy(13))
            .expect("simulator");
        let first = simulator.allocate().expect("first qubit");
        let second = simulator.allocate().expect("second qubit");
        simulator
            .apply(Operation::One {
                gate: OneQubitGate::X,
                target: first,
            })
            .expect("X");
        simulator.swap_ids(first, second).expect("swap IDs");
        assert_eq!(
            simulator
                .apply(Operation::MeasureZ { target: second })
                .expect("measurement"),
            OperationOutcome::Measurement(Measurement::One)
        );
    }

    #[test]
    fn non_local_gate_fails_before_mutation() {
        let mut simulator = DenseFactory
            .create_simulator(policy(17))
            .expect("simulator");
        let first = simulator.allocate().expect("first qubit");
        simulator.allocate().expect("middle qubit");
        let third = simulator.allocate().expect("third qubit");
        let error = simulator
            .apply(Operation::Two {
                gate: TwoQubitGate::Cx,
                first,
                second: third,
            })
            .expect_err("non-local gate should fail");
        assert!(matches!(error, MpsError::CapabilityNotImplemented(_)));
        let report = simulator.report().expect("report");
        assert_eq!(report.operation_counts.two_qubit, 0);
        assert!((report.state_norm - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn fixed_seed_reproduces_measurement() {
        fn sample() -> Measurement {
            let mut simulator = DenseFactory
                .create_simulator(policy(23))
                .expect("simulator");
            let qubit = simulator.allocate().expect("qubit");
            simulator
                .apply(Operation::One {
                    gate: OneQubitGate::H,
                    target: qubit,
                })
                .expect("H");
            let OperationOutcome::Measurement(result) = simulator
                .apply(Operation::MeasureZ { target: qubit })
                .expect("measurement")
            else {
                panic!("expected a measurement");
            };
            result
        }

        assert_eq!(sample(), sample());
    }
}
