import Std.Math.PI;

@EntryPoint(Adaptive_RI)
operation CliffordIsing() : Result[] {
    // Use specifically tailored parameters to get Clifford only
    // rotation values.
    IsingModel2DEvolution(
        10,
        10,
        PI() / 2.0,
        PI() / 2.0,
        300.0,
        300
    )
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
operation IsingModel2DEvolution(
    N1 : Int,
    N2 : Int,
    J : Double,
    g : Double,
    evolutionTime : Double,
    numberOfSteps : Int
) : Result[] {

    // Allocate qubit grid and structure it as a 2D array.
    use qubits = Qubit[N1 * N2];
    let qubitsAs2D = Std.Arrays.Chunks(N2, qubits);

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

        // All Rzz gates applied in the following two loops commute so they can be
        // applied in any order. To reduce the depth of the algorithm, Rzz gates
        // between horizontal "even" pairs of qubits are applied first - pairs
        // that start at even indices. Then Rzz gates between "odd" pairs are
        // applied. That way all Rzz between horizontal "even" pairs can potentially
        // be done in parallel. Same is true about horizontal "odd"  pairs,
        // vertical "even" pairs and vertical "odd" pairs.

        // Horizontal two-qubit interactions
        for row in 0..N1-1 {
            // Horizontal interactions between "even" pairs
            for col in 0..2..N2-2 {
                Rzz(2.0 * theta_zz, qubitsAs2D[row][col], qubitsAs2D[row][col + 1]);
            }

            // Horizontal interactions between "odd" pairs
            for col in 1..2..N2-2 {
                Rzz(2.0 * theta_zz, qubitsAs2D[row][col], qubitsAs2D[row][col + 1]);
            }
        }

        // Vertical two-qubit interactions
        for col in 0..N2-1 {

            // Vertical interactions between "even" pairs
            for row in 0..2..N1-2 {
                Rzz(2.0 * theta_zz, qubitsAs2D[row][col], qubitsAs2D[row + 1][col]);
            }

            // Vertical interactions between "odd" pairs
            for row in 1..2..N1-2 {
                Rzz(2.0 * theta_zz, qubitsAs2D[row][col], qubitsAs2D[row + 1][col]);
            }

        }

    }

    MResetEachZ(qubits)
}
