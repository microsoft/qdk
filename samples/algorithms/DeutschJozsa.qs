/// # Sample
/// Deutsch–Jozsa Algorithm
///
/// # Description
/// Deutsch–Jozsa is a quantum algorithm that determines whether a given Boolean
/// function 𝑓 is constant (0 on all inputs or 1 on all inputs) or balanced
/// (1 for exactly half of the input domain and 0 for the other half).
///
/// This Q# program implements the Deutsch–Jozsa algorithm.
import Std.Diagnostics.*;
import Std.Math.*;
import Std.Measurement.*;

operation Main() : Bool[] {
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
    // determining the type (constant or balanced) of various functions.
    let functionsToTest = [
        SimpleConstantBoolF,
        SimpleBalancedBoolF,
        ConstantBoolF,
        BalancedBoolF
    ];

    mutable results = [];
    for fn in functionsToTest {
        let isConstant = DeutschJozsa(fn, 5);
        results += [isConstant];
    }

    return results;
}

/// # Summary
/// Validates the implementation of the Deutsch–Jozsa algorithm by comparing
/// the results to the expected values.
/// This also demonstrates how to use the `@Test` attribute to define a test operation.
@Test()
operation ValidateDeutschJozsa() : Unit {
    // To see how tests verify the behavior, try changing either the algorithm or the expected values
    // in the assertion below and observe the test failure.
    let expectedResults = [
        ("SimpleConstantBoolF", true),
        ("SimpleBalancedBoolF", false),
        ("ConstantBoolF", true),
        ("BalancedBoolF", false)
    ];
    let results = Main();
    for idx in 0..Length(expectedResults)-1 {
        let (fnName, expected) = expectedResults[idx];
        let actual = results[idx];
        Fact(actual == expected, $"Function {fnName} was expected to be {(expected ? "constant" | "balanced")}, but was determined to be {(actual ? "constant" | "balanced")}.");
    }
}

/// # Summary
/// This operation implements the DeutschJozsa algorithm.
/// It returns the Boolean value `true` if the function is constant and
/// `false` if it is not.
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
/// A boolean value `true` that indicates that the function is constant and
/// `false` that indicates that the function is balanced.
///
/// # See Also
/// - For details see Section 1.4.3 of Nielsen & Chuang.
///
/// # References
/// - [ *Michael A. Nielsen , Isaac L. Chuang*,
///     Quantum Computation and Quantum Information ]
/// (http://doi.org/10.1017/CBO9780511976667)
operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
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

    // The following for-loop measures all qubits and resets them to the |0〉
    // state so that they can be safely deallocated at the end of the block.
    // The loop also sets `result` to `true` if all measurement results are
    // `Zero`, i.e. if the function is a constant function, and sets
    // `result` to `false` if not, which according to the assumption on 𝑓
    // means that it must be balanced.
    mutable result = true;
    for q in queryRegister {
        if MResetZ(q) == One {
            result = false;
        }
    }

    // Finally, the last qubit, which held the 𝑦-register, is reset.
    Reset(target);
    return result;
}

// Simple constant Boolean function
operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
    X(target);
}

// Simple balanced Boolean function
operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
    CX(args[0], target);
}

// A more complex constant Boolean function.
// It applies X to every input basis vector.
operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
    for i in 0..(2^Length(args)) - 1 {
        ApplyControlledOnInt(i, X, args, target);
    }
}

// A more complex balanced Boolean function.
// It applies X to half of the input basis vectors.
operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
    for i in 0..2..(2^Length(args)) - 1 {
        ApplyControlledOnInt(i, X, args, target);
    }
}
