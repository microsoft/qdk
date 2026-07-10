use binar::Bitwise;
use paulimer::UnitaryOp;
use paulimer::clifford::{Clifford, CliffordMutable, CliffordUnitary};
use paulimer::pauli::{Pauli, PauliBits, PauliUnitary, anti_commutes_with, generic::PhaseExponent};
use paulimer::pauli::{PauliBinaryOps, PauliMutable};
use pauliverse::{OutcomeId, Simulation};
use rand::RngExt;
use rand::{Rng, SeedableRng, rngs::StdRng};

type SparsePauli = paulimer::pauli::SparsePauli;

/// Traditional stabilizer simulation with random measurement outcomes.
///
/// This simulator draws random measurement outcomes as needed during simulation,
/// representing a single execution path through the quantum circuit. Each measurement
/// with a random outcome is sampled and recorded, allowing adaptive circuits and
/// noise injection based on concrete outcome values.
///
/// # Use Cases
///
/// - **Monte Carlo sampling**: Run many independent shots to estimate error rates
/// - **Adaptive circuits**: Runtime measurement outcomes determine subsequent gates
/// - **Dynamic noise injection**: Insert novel noise models based on circuit state
/// - **Debugging circuits**: Trace specific execution paths with concrete outcomes
///
/// # Performance
///
/// - **Complexity**: `O(n_gates × n_qubits²)` worst-case per shot
/// - **Best for**: Few shots or adaptive circuits where next gates depend on outcomes
/// - **Compared to `OutcomeComplete`**: More efficient when `shots << n_random`, where `n_random` is the
///   number of random measurements. `OutcomeComplete` becomes advantageous when you need
///   many samples of the same circuit.
/// - **Space**: `O(n_qubits² + n_measurements)`
///
/// # Examples
///
/// ```
/// use pauliverse::{OutcomeSpecificSimulation, Simulation};
/// use paulimer::{UnitaryOp, SparsePauli};
///
/// // Run multiple shots to collect outcome statistics
/// for _ in 0..10 {
///     let mut sim = OutcomeSpecificSimulation::new_with_random_outcomes(2);
///     sim.unitary_op(UnitaryOp::Hadamard, &[0]);
///     sim.unitary_op(UnitaryOp::ControlledX, &[0, 1]);
///
///     let observable: SparsePauli = "ZI".parse().unwrap();
///     let outcome_id = sim.measure(&observable);
///
///     // Access the concrete measurement outcome
///     if outcome_id < sim.outcome_count() {
///         let _value = sim.outcome_vector()[outcome_id];
///         // Process this shot's outcome for statistics...
///     }
/// }
/// ```
///
/// # Alternatives
///
/// - Use [`crate::OutcomeCompleteSimulation`] when you need all possible outcomes
/// - Use [`crate::OutcomeFreeSimulation`] when outcomes don't matter
/// - Use [`crate::FaultySimulation`] for noisy simulations
#[must_use]
pub struct OutcomeSpecificSimulation {
    clifford: CliffordUnitary, // R
    outcome_vector: Vec<bool>,
    bit_source: Box<dyn Iterator<Item = bool> + Send + Sync>,
    random_outcome_indicator: Vec<bool>, // vec(p), [j] is true iff vec(p)_j = 1/2
    num_random_bits: usize,
    qubit_count: usize,
}

impl std::fmt::Debug for OutcomeSpecificSimulation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutcomeSpecificSimulation")
            .field(
                "state_encoder",
                &self.clifford.clone().resize(self.qubit_count),
            )
            .field("outcome_vector", &self.outcome_vector)
            .field("bit_source", &"<bit source iterator>")
            .field("random_outcome_indicator", &self.random_outcome_indicator)
            .field("num_random_bits", &self.num_random_bits)
            .field("qubit_count", &self.qubit_count)
            .finish()
    }
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
            let mut pauli = observable.clone();
            pauli.mul_assign_right(hint);
            pauli.add_assign_phase_exp(3u8.wrapping_sub(preimage.xz_phase_exponent().raw_value()));
            self.clifford.left_mul_pauli_exp(&pauli);
            self.allocate_random_bit();
            self.apply_conditional_pauli_generic(hint, &[self.outcome_count() - 1], true);
        }
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
                let hint = self.clifford.image_z(pos);
                self.measure_with_hint_generic(observable, &hint);
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

//TODO: move to tests module
#[test]
fn init_test() {
    let outcome_specific_simulation = OutcomeSpecificSimulation::new(2);
    // Verify it initializes correctly with just qubit count
    assert_eq!(outcome_specific_simulation.outcome_vector().len(), 0);
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
