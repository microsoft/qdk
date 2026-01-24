/// # Sample
/// Hidden Shift
///
/// # Description
/// There is a family of problems known as hidden shift problems, in which it
/// is given that two Boolean functions 𝑓 and 𝑔 satisfy the relation
///     𝑔(𝑥) = 𝑓(𝑥 ⊕ 𝑠) for all 𝑥
/// where 𝑠 is a hidden bit string that we would like to find.
///
/// This Q# program implements an algorithm to solve the hidden shift problem.
import Std.Arrays.*;
import Std.Convert.*;
import Std.Diagnostics.*;
import Std.Measurement.*;

operation Main() : Result[] {
    // Consider the case of finding a hidden shift 𝑠 between two Boolean
    // functions 𝑓(𝑥) and 𝑔(𝑥) = 𝑓(𝑥 ⊕ 𝑠).
    // This problem can be solved on a quantum computer with one call to
    // each of 𝑓 and 𝑔 in the special case that both functions are bent;
    // that is, that they are as far from linear as possible.

    // Here, we find the hidden shift encoded in the following array of
    // Booleans.
    let shiftAsBoolArray = [true, false, false, false, false, true];
    let shiftAsInt = BoolArrayAsInt(shiftAsBoolArray);
    let hiddenShiftBitString = FindHiddenShift(
        BentFunction,
        register => ShiftedBentFunction(shiftAsInt, register),
        Length(shiftAsBoolArray)
    );

    return hiddenShiftBitString;
}

/// # Summary
/// Implements a correlation-based algorithm to solve the hidden shift
/// problem for bent functions.
///
/// # Description
/// Implements a solution for the hidden shift problem, which is to identify
/// an unknown shift 𝑠 of the arguments of two Boolean functions 𝑓 and 𝑔
/// that are promised to satisfy the relation 𝑔(𝑥) = 𝑓(𝑥 ⊕ 𝑠) for all 𝑥.
///
/// 𝑓 and 𝑔 are assumed to be bent functions. A Boolean function is bent if
/// it is as far from linear as possible. In particular, bent functions have
/// flat Fourier (Walsh–Hadamard) spectra.
///
/// In this case, the Roetteler algorithm (see References, below) uses
/// black-box oracles for 𝑓^* and 𝑔, where 𝑓^* is the dual bent function to
/// 𝑓, and computes the hidden shift 𝑠 between 𝑓 and 𝑔.
///
/// # Input
/// ## Ufstar
/// A quantum operation that implements
/// $U_f^*: |𝑥〉 ↦ (-1)^{f^*(x)} |𝑥〉$,
/// where $f^*$ is a Boolean function, 𝑥 is an $n$ bit register
/// ## Ug
/// A quantum operation that implements
/// $U_g:|𝑥〉 ↦ (-1)^{g(x)} |𝑥〉$,
/// where 𝑔 is a Boolean function that is shifted by unknown
/// 𝑠 from 𝑓, and 𝑥 is an $n$ bit register.
/// ## n
/// The number of bits of the input register |𝑥〉.
///
/// # Output
/// An array of type `Result[]` which encodes the bit representation
/// of the hidden shift.
///
/// # References
/// - [*Martin Roetteler*,
///    Proc. SODA 2010, ACM, pp. 448-457, 2010]
///   (https://doi.org/10.1137/1.9781611973075.37)
operation FindHiddenShift(
    Ufstar : (Qubit[] => Unit),
    Ug : (Qubit[] => Unit),
    n : Int
) : Result[] {
    // We allocate n clean qubits. Note that the function Ufstar and Ug are
    // unitary operations on n qubits defined via phase encoding.
    use qubits = Qubit[n];

    // First, a Hadamard transform is applied to each of the qubits.
    ApplyToEach(H, qubits);

    // We now apply the shifted function Ug to the n qubits, computing
    // |x〉 -> (-1)^{g(x)} |x〉.
    Ug(qubits);

    within {
        // A Hadamard transform is applied to each of the n qubits.
        ApplyToEachA(H, qubits);
    } apply {
        // we now apply the dual function of the unshifted function, i.e.,
        // Ufstar, to the n qubits, computing |x〉 -> (-1)^{fstar(x)} |x〉.
        Ufstar(qubits);
    }

    // Measure the n qubits and reset them to zero so that they can be
    // safely deallocated at the end of the block.
    return MResetEachZ(qubits);
}

/// # Summary
/// Implements an oracle for a bent function constructed from the inner
/// product of Boolean functions.
///
/// # Description
/// This operation defines the Boolean function IP(x_0, ..., x_{n-1}) which
/// is computed into the phase, i.e., a diagonal operator that maps
/// |x〉 -> (-1)^{IP(x)} |x〉, where x stands for x=(x_0, ..., x_{n-1}) and all
/// the x_i are binary. The IP function is defined as
/// IP(y, z) = y_0 z_0 + y_1 z_1 + ... y_{u-1} z_{u-1} where
/// y = (y_0, ..., y_{u-1}) and z =  (z_0, ..., z_{u-1}) are two bit vectors
/// of length u. Notice that the function IP is a Boolean function on n = 2u
/// bits. IP is a special case of bent function. These are functions for
/// which the Walsh-Hadamard transform is perfectly flat (in absolute
/// value).
/// Because of this flatness, the Walsh-Hadamard spectrum of any bent
/// function defines a +1/-1 function, i.e., gives rise to another Boolean
/// function, called the dual bent function. Moreover, for the case of the
/// IP function it can be shown that IP is equal to its own dual bent
/// function.
///
/// # Remarks
/// Notice that a diagonal operator implementing IP between 2 variables y_0
/// and z_0 is nothing but the AND function between those variables, i.e.,
/// in phase encoding it is computed by a Controlled-Z gate.
/// Extending this to an XOR of the AND of more variables, as required in
/// the definition of the IP function can then be accomplished by applying
/// several Controlled-Z gates between the respective inputs.
operation BentFunction(register : Qubit[]) : Unit {
    Fact(Length(register) % 2 == 0, "Length of register must be even.");
    let u = Length(register) / 2;
    let xs = register[0..u - 1];
    let ys = register[u...];
    for index in 0..u - 1 {
        CZ(xs[index], ys[index]);
    }
}

/// # Summary
/// Implements a shifted bend function 𝑔(𝑥) = 𝑓(𝑥 ⊕ 𝑠).
///
/// # Description
/// For the hidden shift problem we need another function g which is related
/// to IP via g(x) = IP(x + s), i.e., we have to shift the argument of the
/// IP function by a given shift. Notice that the '+' operation here is the
/// Boolean addition, i.e., a bit-wise operation. Notice further, that in
/// general a diagonal operation |x〉 -> (-1)^{f(x)} can be turned into a
/// shifted version by applying a bit flip to the |x〉 register first, then
/// applying the diagonal operation, and then undoing the bit flips to the
/// |x〉 register. We use this principle to define shifted versions of the IP
/// operation.
operation ShiftedBentFunction(shift : Int, register : Qubit[]) : Unit {
    Fact(Length(register) % 2 == 0, "Length of register must be even.");
    let u = Length(register) / 2;
    within {
        // Flips the bits in shift.
        ApplyXorInPlace(shift, register);
    } apply {
        // Compute the IP function into the phase.
        BentFunction(register);
    }
}
