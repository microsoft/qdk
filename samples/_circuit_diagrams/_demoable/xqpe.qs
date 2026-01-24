import Std.Diagnostics.DumpMachine;
import Std.Math.ArcCos;
import Std.Math.PI;
import Std.Convert.IntAsDouble;
import Std.Arrays.Subarray;
import Std.StatePreparation.PreparePureStateD;

@EntryPoint(Adaptive_RIF)
operation Main() : Double {
    // Run with the initial quantum state |ψ⟩ = 0.8|00⟩ + 0.6|11⟩.
    // This state is close to the Bell state |Φ+⟩ = (|00⟩+|11⟩)/√2, which is an
    // eigenstate of H = XX + ZZ with eigenvalue E = 2. The high overlap (~0.99)
    // ensures the QPE primarily measures this eigenvalue and returns 2.0 with high probability.
    IQPEMSB(2, 4, [0, 1], [0.8, 0.0, 0.0, 0.6], [], [[PauliX, PauliX], [PauliZ, PauliZ]], PI() / 2.0, "repeat")
}

operation IQPEMSB(
    numQubits : Int,
    numIterations : Int,
    rowMap : Int[],
    stateVector : Double[],
    expansionOps : Int[][],
    pauliExponents : Pauli[][],
    evolutionTime : Double,
    strategy : String,
) : Double {
    mutable accumulatedPhase = 0.0;

    // Perform IQPE iterations
    for k in numIterations.. -1..1 {
        // Allocate qubits
        use ancilla = Qubit();
        use system = Qubit[numQubits];

        // Prepare the initial sparse state
        PrepareSparseState(rowMap, stateVector, expansionOps, system);

        IQPEMSBIteration(pauliExponents, evolutionTime, k, accumulatedPhase, strategy, ancilla, system);

        // Measure the ancilla qubit
        let result = MResetZ(ancilla);
        accumulatedPhase /= 2.0;
        if result == One {
            accumulatedPhase += PI() / 2.0;
        }

        // Reset system qubits
        ResetAll(system);
    }

    return (2.0 * PI() / evolutionTime) * (accumulatedPhase / PI());
}

operation PrepareSparseState(
    rowMap : Int[],
    stateVector : Double[],
    expansionOps : Int[][],
    qs : Qubit[]
) : Unit {
    PreparePureStateD(stateVector, Subarray(rowMap, qs));
    for op in expansionOps {
        if Length(op) == 2 {
            CNOT(qs[op[0]], qs[op[1]]);
        } elif Length(op) == 1 {
            X(qs[op[0]]);
        } else {
            fail "Unsupported operation length in expansionOps.";
        }
    }
}

operation IQPEMSBIteration(
    pauliExponents : Pauli[][],
    evolutionTime : Double,
    k : Int,
    accumulatedPhase : Double,
    strategy : String,
    ancilla : Qubit,
    system : Qubit[]
) : Unit {
    // Step 1: Hadamard basis for ancilla
    within {
        H(ancilla);
    } apply {

        // Step 2: Apply phase kickback if not the first iteration
        if accumulatedPhase > 0.0 or accumulatedPhase < 0.0 {
            Rz(accumulatedPhase, ancilla);
        }

        // Step 3: Apply controlled unitary evolution
        let repetitions = 2^(k - 1);
        Message($"Applying controlled evolution with {repetitions} repetitions using strategy '{strategy}'");
        if strategy == "repeat" {
            for i in 1..repetitions {
                ControlledEvolution(pauliExponents, evolutionTime, ancilla, system);
            }
        } elif strategy == "rescaled" {
            ControlledEvolution(pauliExponents, evolutionTime * IntAsDouble(repetitions), ancilla, system);
        } else {
            fail "Invalid strategy. Use 'repeat' or 'rescaled'.";
        }
    }

    // Step 4: Final Hadamard on ancilla, automatically done by 'within ... apply' block
}

operation ControlledEvolution(pauliExponents : Pauli[][], evolutionTime : Double, control : Qubit, system : Qubit[]) : Unit {
    for paulis in pauliExponents {
        Controlled Exp([control], (paulis, -1.0 * evolutionTime, system));
    }
}
