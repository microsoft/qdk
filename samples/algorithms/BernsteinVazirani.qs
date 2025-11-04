/// # Sample
/// Bernstein-Vazirani Algorithm
///
/// # Description
/// The Bernstein-Vazirani algorithm determines the value of a bit string
/// encoded in a function.
///
/// This Q# program implements the Bernstein-Vazirani algorithm.
import Std.Arrays.*;
import Std.Convert.*;
import Std.Diagnostics.*;
import Std.Math.*;
import Std.Measurement.*;

operation Main() : Int[] {
    use q = Qubit();
    CNOT(q, q);
    fail "hi";
    // Consider a function 𝑓(𝑥⃗) on bitstrings 𝑥⃗ = (𝑥₀, …, 𝑥ₙ₋₁) of the form
    //     𝑓(𝑥⃗) ≔ Σᵢ 𝑥ᵢ 𝑟ᵢ
    // where 𝑟⃗ = (𝑟₀, …, 𝑟ₙ₋₁) is an unknown bitstring that determines the
    // parity of 𝑓.

    // The Bernstein–Vazirani algorithm allows determining 𝑟 given a
    // quantum operation that implements
    //     |𝑥〉|𝑦〉 ↦ |𝑥〉|𝑦 ⊕ 𝑓(𝑥)〉.

    // The entry point function of this program, `Main`, shows how to use
    // the `BernsteinVazirani` operation to determine the value of various
    // integers whose bits describe 𝑟.
    let nQubits = 10;

    // Use the Bernstein–Vazirani algorithm to determine the bit strings
    // that various integers represent.
    let integers = [127, 238, 512];
    mutable decodedIntegers = [];
    for integer in integers {
        // Create an operation that encodes a bit string represented by an
        // integer as a parity operation.
        let parityOperation = EncodeIntegerAsParityOperation(integer);

        // Use the parity operation as input to the Bernstein-Vazirani
        // algorithm to determine the bit string.
        let decodedBitString = BernsteinVazirani(parityOperation, nQubits);
        let decodedInteger = ResultArrayAsInt(decodedBitString);
        Fact(
            decodedInteger == integer,
            $"Decoded integer {decodedInteger}, but expected {integer}."
        );

        Message($"Successfully decoded bit string as int: {decodedInteger}");
        decodedIntegers += [decodedInteger];
    }

    return decodedIntegers;
}

/// # Summary
/// This operation implements the Bernstein-Vazirani quantum algorithm.
/// This algorithm computes for a given Boolean function that is promised to
/// be a parity 𝑓(𝑥₀, …, 𝑥ₙ₋₁) = Σᵢ 𝑟ᵢ 𝑥ᵢ a result in the form of a bit
/// vector (𝑟₀, …, 𝑟ₙ₋₁) corresponding to the parity function.
/// Note that it is promised that the function is actually a parity
/// function.
///
/// # Input
/// ## Uf
/// A quantum operation that implements |𝑥〉|𝑦〉 ↦ |𝑥〉|𝑦 ⊕ 𝑓(𝑥)〉,
/// where 𝑓 is a Boolean function that implements a parity Σᵢ 𝑟ᵢ 𝑥ᵢ.
/// ## n
/// The number of bits in the input register |𝑥〉.
///
/// # Output
/// An array of type `Result[]` that contains the parity 𝑟⃗ = (𝑟₀, …, 𝑟ₙ₋₁).
///
/// # See Also
/// - For details see Section 1.4.3 of Nielsen & Chuang.
///
/// # References
/// - [ *Ethan Bernstein and Umesh Vazirani*,
///     SIAM J. Comput., 26(5), 1411–1473, 1997 ]
///   (https://doi.org/10.1137/S0097539796300921)
operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
    // We allocate n + 1 clean qubits. Note that the function Uf is defined
    // on inputs of the form (x, y), where x has n bits and y has 1 bit.
    use queryRegister = Qubit[n];
    use target = Qubit();

    // The last qubit needs to be flipped so that the function will actually
    // be computed into the phase when Uf is applied.
    X(target);

    within {
        // Now, a Hadamard transform is applied to each of the qubits. As
        // the last step before the measurement, a Hadamard transform is
        // applied to all qubits except last one. We could apply the
        // transform to the last qubit also, but this would not affect the
        // final outcome.
        // We use a within-apply block to ensure that the Hadamard transform
        // is correctly inverted.
        ApplyToEachA(H, queryRegister);
    } apply {
        H(target);
        // We now apply Uf to the n+1 qubits, computing
        // |x, y〉 ↦ |x, y ⊕ f(x)〉.
        Uf(queryRegister, target);
    }

    // Measure all qubits and reset them to the |0〉 state so that they can
    // be safely deallocated at the end of the block.
    let resultArray = MResetEachZ(queryRegister);

    // Finally, the last qubit, which held the y-register, is reset.
    Reset(target);

    // The result is already contained in resultArray so no further
    // post-processing is necessary.
    return resultArray;
}

/// # Summary
/// Given an integer that can be represented as a bit string
/// 𝑟⃗ = (r₀, …, rₙ₋₁), this operation applies a unitary 𝑈 that acts on 𝑛 + 1
/// qubits as:
///     𝑈 |𝑥〉|𝑦〉 = |𝑥〉|𝑦 ⊕ 𝑓(𝑥)〉
/// where 𝑓(𝑥) = Σᵢ 𝑥ᵢ 𝑟ᵢ mod 2.
///
/// # Input
/// ## bitStringAsInt
/// An integer that can be represented as a bit string 𝑟⃗ used to define the
/// function 𝑓.
/// ## xRegister
/// Represents the |𝑥〉 register that 𝑈 acts on.
/// ## yQubit
/// Represents the |𝑦〉 qubit that 𝑈 acts on.
operation ApplyParityOperation(
    bitStringAsInt : Int,
    xRegister : Qubit[],
    yQubit : Qubit
) : Unit {
    // `xRegister` muts have enough qubits to represent the integer.
    let requiredBits = BitSizeI(bitStringAsInt);
    let availableQubits = Length(xRegister);
    Fact(
        availableQubits >= requiredBits,
        $"Integer value {bitStringAsInt} requires {requiredBits} bits to be represented but the quantum register only has {availableQubits} qubits"
    );

    // Apply the quantum operations that encode the bit string.
    for index in IndexRange(xRegister) {
        if ((bitStringAsInt &&& 2^index) != 0) {
            CNOT(xRegister[index], yQubit);
        }
    }
}

/// # Summary
/// Returns black-box operations (Qubit[], Qubit) => () of the form
/// U_f |𝑥〉|𝑦〉 = |𝑥〉|𝑦 ⊕ 𝑓(𝑥)〉.
/// We define 𝑓 by providing the bit string 𝑟⃗ as an integer.
function EncodeIntegerAsParityOperation(bitStringAsInt : Int) : (Qubit[], Qubit) => Unit {
    return ApplyParityOperation(bitStringAsInt, _, _);
}
