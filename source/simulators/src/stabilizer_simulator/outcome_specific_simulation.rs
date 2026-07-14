use binar::Bitwise;
use paulimer::{
    UnitaryOp,
    clifford::{Clifford, CliffordMutable, CliffordUnitary},
    pauli::{
        Pauli, PauliBinaryOps, PauliBits, PauliMutable, PauliUnitary, anti_commutes_with,
        generic::PhaseExponent,
    },
};
use pauliverse::{OutcomeId, Simulation};
use rand::{Rng, RngExt, SeedableRng, rngs::StdRng};

type SparsePauli = paulimer::pauli::SparsePauli;

#[must_use]
pub struct OutcomeSpecificSimulation {
    clifford: CliffordUnitary,
    outcome_vector: Vec<bool>,
    bit_source: Box<dyn Iterator<Item = bool> + Send + Sync>,
    random_outcome_indicator: Vec<bool>,
    num_random_bits: usize,
    qubit_count: usize,
}

impl Default for OutcomeSpecificSimulation {
    fn default() -> Self {
        OutcomeSpecificSimulation::with_capacity(0, 0, 0)
    }
}

impl OutcomeSpecificSimulation {
    fn ensure_qubit_capacity(&mut self, max_qubit_id: Option<usize>) {
        if let Some(max_qubit_id) = max_qubit_id {
            self.qubit_count = std::cmp::max(self.qubit_count, max_qubit_id + 1);
            if max_qubit_id >= self.qubit_capacity() {
                let new_capacity = (max_qubit_id + 1).next_power_of_two();
                self.reserve_qubits(new_capacity);
            }
        }
    }

    /// Create a simulation with a custom source for random outcome bits.
    ///
    /// The `bit_source` iterator provides outcome values for random measurements.
    /// This allows deterministic testing or custom random number generation.
    pub fn new_with_bit_source(
        num_qubits: usize,
        bit_source: impl Iterator<Item = bool> + 'static + Send + Sync,
    ) -> Self {
        OutcomeSpecificSimulation {
            clifford: CliffordUnitary::identity(num_qubits),
            outcome_vector: Vec::new(),
            bit_source: Box::new(bit_source),
            random_outcome_indicator: Vec::new(),
            num_random_bits: 0,
            qubit_count: num_qubits,
        }
    }

    /// Create a simulation with custom bit source and pre-allocated capacity.
    pub fn with_bit_source_and_capacity(
        num_qubits: usize,
        bit_source: impl Iterator<Item = bool> + 'static + Send + Sync,
        num_outcomes: usize,
    ) -> Self {
        OutcomeSpecificSimulation {
            clifford: CliffordUnitary::identity(num_qubits),
            outcome_vector: Vec::with_capacity(num_outcomes),
            bit_source: Box::new(bit_source),
            random_outcome_indicator: Vec::with_capacity(num_outcomes),
            num_random_bits: 0,
            qubit_count: num_qubits,
        }
    }

    /// Create a simulation with thread-local random number generation.
    ///
    /// This is the standard constructor for Monte Carlo sampling.
    pub fn new_with_random_outcomes(num_qubits: usize) -> Self {
        Self::new_with_bit_source(
            num_qubits,
            SeededRandomBitIterator::new(rand::rng().next_u64()),
        )
    }

    /// Create a simulation with seeded random number generation.
    ///
    /// Useful for reproducible simulations and testing.
    pub fn new_with_seeded_random_outcomes(num_qubits: usize, seed: u64) -> Self {
        Self::new_with_bit_source(num_qubits, SeededRandomBitIterator::new(seed))
    }

    /// Create a simulation where all random outcomes are zero.
    ///
    /// Useful for testing and debugging specific execution paths.
    pub fn new_with_zero_outcomes(num_qubits: usize) -> Self {
        Self::new_with_bit_source(num_qubits, ZeroBitIterator)
    }

    /// Create a simulation with zero outcomes and pre-allocated capacity.
    pub fn with_zero_outcomes_and_capacity(num_qubits: usize, num_outcomes: usize) -> Self {
        Self::with_bit_source_and_capacity(num_qubits, ZeroBitIterator, num_outcomes)
    }

    /// Get the Clifford unitary encoding the current stabilizer state.
    ///
    /// This is the unitary R such that R|0⟩ equals the current state.
    pub fn state_encoder(&self) -> CliffordUnitary {
        let mut res = self.clifford.clone();
        res.resize(self.qubit_count);
        res
    }

    /// Get the vector of measurement outcome values.
    ///
    /// Returns a slice where `[i]` is the boolean value of outcome `i`.
    #[must_use]
    pub fn outcome_vector(&self) -> &Vec<bool> {
        &self.outcome_vector
    }

    pub fn with_capacity(
        num_qubits: usize,
        num_outcomes: usize,
        _num_random_outcomes: usize,
    ) -> Self {
        Self::with_bit_source_and_capacity(
            num_qubits,
            SeededRandomBitIterator::new(rand::rng().next_u64()),
            num_outcomes,
        )
    }

    pub fn new(num_qubits: usize) -> Self {
        Self::new_with_random_outcomes(num_qubits)
    }

    /// # Panics
    /// Panics if `hint` commutes with `observable`
    fn measure_with_hint_generic<HintBits: PauliBits, HintPhase: PhaseExponent>(
        &mut self,
        observable: &SparsePauli,
        hint: &PauliUnitary<HintBits, HintPhase>,
    ) {
        assert!(
            anti_commutes_with(observable, hint),
            "observable={observable}, hint={hint}"
        );

        let preimage = self.clifford.preimage(hint);
        if preimage.x_bits().support().next().is_some() {
            // hint is not true
            self.measure(observable);
        } else {
            self.apply_random_measurement(
                observable,
                hint,
                preimage.xz_phase_exponent().raw_value(),
            );
        }
    }

    /// Update the stabilizer frame for a random measurement whose anti-commuting `hint`
    /// has a `preimage` with empty X-support and the given raw phase exponent.
    ///
    /// `preimage_phase_raw` is the raw phase exponent of `preimage(hint)`. Callers that
    /// already know this value (see [`Simulation::measure`], where `hint = image_z(pos)`
    /// implies `preimage(hint) == Z_pos` with phase exponent `0`) can pass it directly to
    /// avoid recomputing the dense preimage.
    fn apply_random_measurement<HintBits: PauliBits, HintPhase: PhaseExponent>(
        &mut self,
        observable: &SparsePauli,
        hint: &PauliUnitary<HintBits, HintPhase>,
        preimage_phase_raw: u8,
    ) {
        let mut pauli = observable.clone();
        pauli.mul_assign_right(hint);
        pauli.add_assign_phase_exp(3u8.wrapping_sub(preimage_phase_raw));
        self.clifford.left_mul_pauli_exp(&pauli);
        self.allocate_random_bit();
        self.apply_conditional_pauli_generic(hint, &[self.outcome_count() - 1], true);
    }

    fn measure_deterministic<Bits: PauliBits, Phase: PhaseExponent>(
        &mut self,
        preimage: &PauliUnitary<Bits, Phase>,
    ) {
        debug_assert!(preimage.xz_phase_exponent().is_even());
        self.outcome_vector
            .push(preimage.xz_phase_exponent().value() == 2);
        self.random_outcome_indicator.push(false);
    }

    fn apply_conditional_pauli_generic<Bits: PauliBits, Phase: PhaseExponent>(
        &mut self,
        pauli: &PauliUnitary<Bits, Phase>,
        outcomes_indicator: &[usize],
        parity: bool,
    ) {
        if total_parity(self.outcome_vector(), outcomes_indicator) == parity {
            self.clifford.left_mul_pauli(pauli);
        }
    }

    /// Get the number of random (non-deterministic) measurement outcomes.
    #[must_use]
    pub fn random_outcome_count(&self) -> usize {
        self.num_random_bits
    }

    /// Get indicators for which outcomes are random.
    ///
    /// Returns a slice where `[i]` is true if outcome `i` was random.
    #[must_use]
    pub fn random_outcome_indicator(&self) -> &[bool] {
        &self.random_outcome_indicator
    }
}

impl OutcomeSpecificSimulation {
    /// Force a Z-basis measurement on `index` to have the requested `value`.
    pub fn post_select_z(&mut self, value: bool, index: usize) -> Result<(), String> {
        self.ensure_qubit_capacity(Some(index));
        let observable = SparsePauli::from([paulimer::core::z(index)]);
        let preimage = self.clifford.preimage(&observable);

        if let Some(pos) = preimage.x_bits().support().next() {
            let hint = self.clifford.image_z(pos);
            let mut pauli = observable.clone() * &hint;
            pauli.add_assign_phase_exp(3u8);
            self.clifford.left_mul_pauli_exp(&pauli);

            self.outcome_vector.push(value);
            self.random_outcome_indicator.push(true);
            self.num_random_bits += 1;
            self.apply_conditional_pauli_generic(&hint, &[self.outcome_count() - 1], true);
        } else {
            self.measure_deterministic(&preimage);
            if self.outcome_vector.last() != Some(&value) {
                return Err("post-selection condition has zero probability".into());
            }
        }

        Ok(())
    }
}

impl Simulation for OutcomeSpecificSimulation {
    fn allocate_random_bit(&mut self) -> usize {
        let random_bit = self
            .bit_source
            .next()
            .expect("Bit source iterator should be infinite");

        self.outcome_vector.push(random_bit);
        self.random_outcome_indicator.push(true);
        self.num_random_bits += 1;
        self.num_random_bits - 1
    }

    fn conditional_pauli(
        &mut self,
        observable: &SparsePauli,
        outcomes: &[OutcomeId],
        parity: bool,
    ) {
        self.apply_conditional_pauli_generic(observable, outcomes, parity);
    }

    fn clifford(&mut self, clifford: &CliffordUnitary, support: &[usize]) {
        self.ensure_qubit_capacity(max_support(support));
        self.clifford.left_mul_clifford(clifford, support);
    }

    fn unitary_op(&mut self, unitary_op: UnitaryOp, support: &[usize]) {
        self.ensure_qubit_capacity(max_support(support));
        let clifford = &mut self.clifford;
        clifford.left_mul(unitary_op, support);
    }

    fn permute(&mut self, permutation: &[usize], support: &[usize]) {
        self.ensure_qubit_capacity(max_support(support));
        self.clifford.left_mul_permutation(permutation, support);
    }

    fn controlled_pauli(&mut self, observable1: &SparsePauli, observable2: &SparsePauli) {
        self.ensure_qubit_capacity(max_pair_support(observable1, observable2));
        self.clifford
            .left_mul_controlled_pauli(observable1, observable2);
    }

    fn pauli(&mut self, observable: &SparsePauli) {
        self.ensure_qubit_capacity(observable.max_support());
        self.clifford.left_mul_pauli(observable);
    }

    fn pauli_exp(&mut self, observable: &SparsePauli) {
        self.ensure_qubit_capacity(observable.max_support());
        self.clifford.left_mul_pauli_exp(observable);
    }

    fn is_stabilizer_up_to_sign(&self, observable: &SparsePauli) -> bool {
        self.clifford.preimage(observable).x_bits().is_zero()
    }

    fn qubit_count(&self) -> usize {
        self.qubit_count
    }

    fn is_stabilizer(&self, observable: &SparsePauli) -> bool {
        let preimage = self.clifford.preimage(observable);
        preimage.x_bits().weight() == 0 && preimage.xz_phase_exponent().value() == 0
    }

    fn is_stabilizer_with_conditional_sign(
        &self,
        observable: &SparsePauli,
        outcomes: &[OutcomeId],
    ) -> bool {
        let parity = total_parity(self.outcome_vector(), outcomes);
        let preimage = self.clifford.preimage(observable);
        preimage.x_bits().weight() == 0 && (preimage.xz_phase_exponent().value() == 0) != parity
    }

    fn measure(&mut self, observable: &SparsePauli) -> OutcomeId {
        self.ensure_qubit_capacity(observable.max_support());
        let preimage = self.clifford.preimage(observable);
        let non_zero_pos = preimage.x_bits().support().next();
        match non_zero_pos {
            Some(pos) => {
                // `pos` lies in the X-support of `preimage(observable)`, so `hint =
                // image_z(pos)` necessarily anti-commutes with `observable`. Moreover,
                // `preimage(hint) = preimage(image_z(pos)) = Z_pos`, which always has empty
                // X-support and phase exponent 0. We can therefore skip the redundant dense
                // `preimage(hint)` computed by `measure_with_hint_generic` and update the
                // stabilizer frame directly.
                let hint = self.clifford.image_z(pos);
                debug_assert!(
                    {
                        let preimage_hint = self.clifford.preimage(&hint);
                        preimage_hint.x_bits().support().next().is_none()
                            && preimage_hint.xz_phase_exponent().raw_value() == 0
                    },
                    "preimage(image_z(pos)) must be Z_pos with phase exponent 0"
                );
                self.apply_random_measurement(observable, &hint, 0);
            }
            None => {
                self.measure_deterministic(&preimage);
            }
        }
        self.outcome_count() - 1
    }

    fn measure_with_hint(&mut self, observable: &SparsePauli, hint: &SparsePauli) -> OutcomeId {
        self.ensure_qubit_capacity(max_pair_support(observable, hint));
        self.measure_with_hint_generic(observable, hint);
        self.outcome_vector().len() - 1
    }

    fn outcome_count(&self) -> usize {
        self.random_outcome_indicator.len()
    }

    fn with_capacity(qubit_count: usize, outcome_count: usize, random_outcome_count: usize) -> Self
    where
        Self: Sized,
    {
        OutcomeSpecificSimulation::with_capacity(qubit_count, outcome_count, random_outcome_count)
    }

    fn qubit_capacity(&self) -> usize {
        self.clifford.num_qubits()
    }

    fn reserve_qubits(&mut self, new_capacity: usize) {
        if new_capacity > self.qubit_capacity() {
            self.clifford.resize(new_capacity);
        }
    }

    fn outcome_capacity(&self) -> usize {
        self.random_outcome_indicator.capacity()
    }

    fn random_outcome_capacity(&self) -> usize {
        self.outcome_capacity()
    }

    fn reserve_outcomes(
        &mut self,
        new_outcome_capacity: usize,
        new_random_outcome_capacity: usize,
    ) {
        let new_capacity = new_outcome_capacity.max(new_random_outcome_capacity);
        if new_capacity > self.outcome_capacity() {
            self.outcome_vector
                .reserve(new_outcome_capacity - self.outcome_capacity());
            self.random_outcome_indicator
                .reserve(new_outcome_capacity - self.outcome_capacity());
        }
    }
}

fn total_parity(outcome_vector: &[bool], outcomes_indicator: &[usize]) -> bool {
    let mut res = false;
    for j in outcomes_indicator {
        res ^= outcome_vector[*j];
    }
    res
}

pub struct RandomBitIterator {
    rng: rand::rngs::ThreadRng,
}

impl RandomBitIterator {
    #[must_use]
    pub fn new() -> Self {
        Self { rng: rand::rng() }
    }
}

impl Default for RandomBitIterator {
    fn default() -> Self {
        Self::new()
    }
}

impl Iterator for RandomBitIterator {
    type Item = bool;

    fn next(&mut self) -> Option<bool> {
        Some(self.rng.random::<bool>())
    }
}

pub struct SeededRandomBitIterator {
    rng: StdRng,
}

impl SeededRandomBitIterator {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
        }
    }
}

impl Iterator for SeededRandomBitIterator {
    type Item = bool;

    fn next(&mut self) -> Option<bool> {
        Some(self.rng.random::<bool>())
    }
}

/// Iterator that always returns false
pub struct ZeroBitIterator;

impl Iterator for ZeroBitIterator {
    type Item = bool;

    fn next(&mut self) -> Option<bool> {
        Some(false)
    }
}

#[must_use]
pub fn max_support(support: &[usize]) -> Option<usize> {
    support.iter().copied().max()
}

pub fn max_pair_support<PauliLike1: Pauli, PauliLike2: Pauli>(
    a: &PauliLike1,
    b: &PauliLike2,
) -> Option<usize> {
    match (a.max_support(), b.max_support()) {
        (None, None) => None,
        (Some(id), None) | (None, Some(id)) => Some(id),
        (Some(id1), Some(id2)) => Some(std::cmp::max(id1, id2)),
    }
}

#[cfg(test)]
mod measure_tests {
    use super::OutcomeSpecificSimulation;
    use paulimer::UnitaryOp;
    use pauliverse::Simulation;
    use rand::{RngExt, SeedableRng, rngs::StdRng};

    type SparsePauli = paulimer::pauli::SparsePauli;

    fn z_obs(qubit: usize) -> SparsePauli {
        [paulimer::core::z(qubit)].into()
    }

    /// Exercises many random Clifford circuits with interleaved random measurements.
    ///
    /// The debug assertion inside `OutcomeSpecificSimulation::measure` verifies that
    /// `preimage(image_z(pos)) == Z_pos` (empty X-support, phase exponent 0). Those are
    /// exactly the two facts that make the fast measurement path equivalent to the general
    /// `measure_with_hint_generic` path, so if this test passes in a debug build the
    /// optimization is behavior-preserving.
    #[test]
    fn fast_measure_path_matches_general_path() {
        let mut rng = StdRng::seed_from_u64(0x00DD_BA11);
        for trial in 0..300 {
            let n = 9;
            let mut sim = OutcomeSpecificSimulation::new_with_seeded_random_outcomes(n, trial);
            for _ in 0..80 {
                match rng.random_range(0..6) {
                    0 => sim.unitary_op(UnitaryOp::Hadamard, &[rng.random_range(0..n)]),
                    1 => sim.unitary_op(UnitaryOp::SqrtZ, &[rng.random_range(0..n)]),
                    2 => sim.unitary_op(UnitaryOp::X, &[rng.random_range(0..n)]),
                    3 => sim.unitary_op(UnitaryOp::Z, &[rng.random_range(0..n)]),
                    4 => {
                        let a = rng.random_range(0..n);
                        let mut b = rng.random_range(0..n);
                        while b == a {
                            b = rng.random_range(0..n);
                        }
                        sim.unitary_op(UnitaryOp::ControlledX, &[a, b]);
                    }
                    _ => {
                        sim.measure(&z_obs(rng.random_range(0..n)));
                    }
                }
            }
        }
    }

    /// End-to-end correctness check independent of the internal fast-path assumption:
    /// a Bell state `(H0, CX01)` must yield perfectly correlated `Z0`/`Z1` outcomes.
    #[test]
    fn bell_state_outcomes_are_correlated() {
        for seed in 0..200 {
            let mut sim = OutcomeSpecificSimulation::new_with_seeded_random_outcomes(2, seed);
            sim.unitary_op(UnitaryOp::Hadamard, &[0]);
            sim.unitary_op(UnitaryOp::ControlledX, &[0, 1]);
            let m0 = sim.measure(&z_obs(0));
            let m1 = sim.measure(&z_obs(1));
            assert_eq!(
                sim.outcome_vector()[m0],
                sim.outcome_vector()[m1],
                "Bell-state Z0/Z1 outcomes must match (seed={seed})"
            );
        }
    }

    /// End-to-end correctness check: an `n`-qubit GHZ state must yield identical outcomes
    /// on every qubit, and re-measuring the same qubit must be deterministic.
    #[test]
    fn ghz_state_outcomes_agree_and_are_repeatable() {
        for seed in 0..100 {
            let n = 6;
            let mut sim = OutcomeSpecificSimulation::new_with_seeded_random_outcomes(n, seed);
            sim.unitary_op(UnitaryOp::Hadamard, &[0]);
            for q in 1..n {
                sim.unitary_op(UnitaryOp::ControlledX, &[0, q]);
            }
            let first = sim.measure(&z_obs(0));
            let first_val = sim.outcome_vector()[first];
            for q in 1..n {
                let m = sim.measure(&z_obs(q));
                assert_eq!(
                    sim.outcome_vector()[m],
                    first_val,
                    "GHZ qubit {q} outcome must match qubit 0 (seed={seed})"
                );
            }
            // Re-measuring qubit 0 is now deterministic and must reproduce the value.
            let again = sim.measure(&z_obs(0));
            assert_eq!(sim.outcome_vector()[again], first_val);
        }
    }
}

#[cfg(test)]
mod post_select_tests {
    use super::OutcomeSpecificSimulation;
    use paulimer::UnitaryOp;
    use pauliverse::Simulation;

    type SparsePauli = paulimer::pauli::SparsePauli;

    fn z_obs(qubit: usize) -> SparsePauli {
        [paulimer::core::z(qubit)].into()
    }

    #[test]
    fn random_post_selection_collapses_bell_state() {
        for value in [false, true] {
            let mut sim = OutcomeSpecificSimulation::new_with_bit_source(2, std::iter::empty());
            sim.unitary_op(UnitaryOp::Hadamard, &[0]);
            sim.unitary_op(UnitaryOp::ControlledX, &[0, 1]);

            sim.post_select_z(value, 0).expect("selection is possible");

            assert_eq!(sim.outcome_vector(), &[value]);
            assert_eq!(sim.random_outcome_indicator(), &[true]);
            assert_eq!(sim.random_outcome_count(), 1);

            let correlated = sim.measure(&z_obs(1));
            assert_eq!(sim.outcome_vector()[correlated], value);
            assert!(!sim.random_outcome_indicator()[correlated]);
        }
    }

    #[test]
    fn deterministic_post_selection_rejects_impossible_value() {
        let mut accepted = OutcomeSpecificSimulation::new_with_zero_outcomes(1);
        accepted
            .post_select_z(false, 0)
            .expect("zero state has Z outcome false");
        assert_eq!(accepted.outcome_vector(), &[false]);
        assert_eq!(accepted.random_outcome_indicator(), &[false]);
        assert_eq!(accepted.random_outcome_count(), 0);

        let mut rejected = OutcomeSpecificSimulation::new_with_zero_outcomes(1);
        let error = rejected
            .post_select_z(true, 0)
            .expect_err("zero state cannot have Z outcome true");
        assert_eq!(error, "post-selection condition has zero probability");
        assert_eq!(rejected.outcome_vector(), &[false]);
        assert_eq!(rejected.random_outcome_indicator(), &[false]);
        assert_eq!(rejected.random_outcome_count(), 0);
    }
}
