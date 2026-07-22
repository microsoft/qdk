// # Sample
// Simulation of a simple Ising model evolution
// on a 1D grid with first-order Trotterization.
//
// # Description
// This sample demonstrates simulation of an Ising model Hamiltonian
// on 1D grid of size N using a first-order Trotter-Suzuki approximation.
// This sample can be easily simulated classically with the grid of size 9
// and 1000 shots. This sample is suitable for Base Profile.
// For the purpose of simplicity this sample intentionally doesn't
// post-process results or perform eigenvalue estimation.
operation Main() : Result[] {
    // The size of a 1D grid is N
    let N : Int = 9;

    // Total evolution time
    let evolutionTime : Double = 4.0;
    // Number of steps
    let numberOfSteps : Int = 7;

    // Coefficient for 2-qubit interactions between neighboring qubits
    let J : Double = 1.0;
    // Coefficient for external field interaction for individual qubits
    let g : Double = 0.7;

    IsingModel1DEvolution(N, J, g, evolutionTime, numberOfSteps)
}

/// # Summary
/// Simulate simple Ising model evolution
///
/// # Description
/// Simulates state |𝜓⟩ evolution to find |𝜓(t)⟩=U(t)|𝜓(0)⟩.
/// |𝜓(0)⟩ is taken to be |0...0⟩.
/// U(t)=e⁻ⁱᴴᵗ, where H is an Ising model Hamiltonian H = -J·Σ'ᵢⱼZᵢZⱼ + g·ΣᵢXᵢ
/// Here Σ' is taken over all pairs of neighboring qubits <i,j>.
/// Simulation is done by performing K steps assuming U(t)≈(U(t/K))ᴷ.
operation IsingModel1DEvolution(
    N : Int,
    J : Double,
    g : Double,
    evolutionTime : Double,
    numberOfSteps : Int
) : Result[] {

    // Allocate qubit grid
    use qubits = Qubit[N];

    // Compute the time step
    let dt : Double = evolutionTime / Std.Convert.IntAsDouble(numberOfSteps);

    let theta_x = - g * dt;
    let theta_zz = J * dt;

    // Perform K steps
    for i in 1..numberOfSteps {

        // Single-qubit interaction with external field
        for q in qubits {
            Rx(2.0 * theta_x, q);
        }

        // All of the following Rzz gates commute. So we apply them between "even"
        // pairs first and then between "odd" pairs to reduce the algorithm depth.

        // Interactions between "even" pairs
        for j in 0..2..N-2 {
            Rzz(2.0 * theta_zz, qubits[j], qubits[j + 1]);
        }

        // Interactions between "odd" pairs
        for j in 1..2..N-2 {
            Rzz(2.0 * theta_zz, qubits[j], qubits[j + 1]);
        }

    }

    MResetEachZ(qubits)
}
