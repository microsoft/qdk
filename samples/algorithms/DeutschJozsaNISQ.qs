// # Sample
// Deutsch–Jozsa Algorithm
//
// # Description
// Deutsch–Jozsa is a quantum algorithm that determines whether a given Boolean
// function 𝑓 is constant (0 on all inputs or 1 on all inputs) or balanced
// (1 for exactly half of the input domain and 0 for the other half).
//
// This Q# program implements the Deutsch–Jozsa algorithm.
import Std.Measurement.*;

operation Main() : (Result[], Result[]) {
    // A Boolean function is a function that maps bitstrings to a bit:
    //     𝑓 : {0, 1}^n → {0, 1}.

    // We say that 𝑓 is constant if 𝑓(𝑥⃗) = 𝑓(𝑦⃗) for all bitstrings 𝑥⃗ and
    // 𝑦⃗, and that 𝑓 is balanced if 𝑓 evaluates to true for exactly half of
    // its inputs.

    // If we are given a function 𝑓 as a quantum operation 𝑈 |𝑥〉|𝑦〉 =
    // |𝑥〉|𝑦 ⊕ 𝑓(𝑥)〉, and are promised that 𝑓 is either constant or is
    // balanced, then the Deutsch–Jozsa algorithm decides between these
    // cases with a single application of 𝑈.

    // Here, we demonstrate the use of the Deutsch-Jozsa algorithm by
    // determining the type (constant or balanced) of a couple of functions.
    let balancedResults = DeutschJozsa(SimpleBalancedBoolF, 5);
    let constantResults = DeutschJozsa(SimpleConstantBoolF, 5);
    return (balancedResults, constantResults);
}

/// # Summary
/// This operation implements the DeutschJozsa algorithm.
/// It returns the query register measurement results. If all the measurement
/// results are `Zero`, the function is constant. If at least one measurement
/// result is `One`, the function is balanced.
/// It is assumed that the function is either constant or balanced.
///
/// # Input
/// ## Uf
/// A quantum operation that implements |𝑥〉|𝑦〉 ↦ |𝑥〉|𝑦 ⊕ 𝑓(𝑥)〉, where 𝑓 is a
/// Boolean function, 𝑥 is an 𝑛 bit register and 𝑦 is a single qubit.
/// ## n
/// The number of bits in the input register |𝑥〉.
///
/// # Output
/// An array of measurement results for the query register.
/// All `Zero` measurement results indicate that the function is constant.
/// At least one `One` measurement result in the array indicates that the
/// function is balanced.
///
/// # See Also
/// - For details see Section 1.4.3 of Nielsen & Chuang.
///
/// # References
/// - [ *Michael A. Nielsen , Isaac L. Chuang*,
///     Quantum Computation and Quantum Information ]
/// (http://doi.org/10.1017/CBO9780511976667)
operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
    // We allocate n + 1 clean qubits. Note that the function `Uf` is defined
    // on inputs of the form (x, y), where x has n bits and y has 1 bit.
    use queryRegister = Qubit[n];
    use target = Qubit();

    // The last qubit needs to be flipped so that the function will actually
    // be computed into the phase when Uf is applied.
    X(target);

    // Now, a Hadamard transform is applied to each of the qubits.
    H(target);
    // We use a within-apply block to ensure that the Hadamard transform is
    // correctly inverted on the |𝑥〉 register.
    within {
        for q in queryRegister {
            H(q);
        }
    } apply {
        // We apply Uf to the n+1 qubits, computing |𝑥, 𝑦〉 ↦ |𝑥, 𝑦 ⊕ 𝑓(𝑥)〉.
        Uf(queryRegister, target);
    }

    // Measure the query register and reset all qubits so they can be safely
    // deallocated.
    let results = MResetEachZ(queryRegister);
    Reset(target);
    return results;
}

// Simple constant Boolean function
operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
    X(target);
}

// Simple balanced Boolean function
operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
    CX(args[0], target);
}
