// OpenQASM Bernstein-Vazirani sample
//
// This sample demonstrates the Bernstein-Vazirani algorithm,
// which determines the value of a bit string encoded in a function.

OPENQASM 3;
include "stdgates.inc";

// Define the number of qubits.
const int nQubits = 5;
// The secret bit string to be determined.
const bit[nQubits] secretBitString = "10101";

// Given bit string 𝑟⃗ = (r₀, …, rₙ₋₁), represented as an array of bits,
// this operation applies a unitary 𝑈 that acts on 𝑛 + 1 qubits as:
//     𝑈 |𝑥〉|𝑦〉 = |𝑥〉|𝑦 ⊕ 𝑓(𝑥)〉
// where 𝑓(𝑥) = Σᵢ 𝑥ᵢ 𝑟ᵢ mod 2.
def ApplyParityOperation(
    bit[nQubits] bitStringAsBoolArray,
    qubit[nQubits] xRegister,
    qubit yQubit ) {

    // Apply the quantum operations that encode the secret bit string.
    for int i in [0:nQubits-1] {
        if (bitStringAsBoolArray[i]) {
            cx xRegister[i], yQubit;
        }
    }
}

// Applies parity operation for a particular secret bit string.
def ParityOperationForSecretBitstring(qubit[nQubits] xRegister, qubit yQubit) {
    ApplyParityOperation(secretBitString, xRegister, yQubit);
}

// Given a register in the all-zeros state, prepares a uniform
// superposition over all basis states.
def PrepareUniform(qubit[nQubits] q) {
    for int i in [0:nQubits-1] {
        h q[i];
    }
}

// This operation implements the Bernstein-Vazirani quantum algorithm.
// This algorithm computes for a given Boolean function that is promised to
// be a parity 𝑓(𝑥₀, …, 𝑥ₙ₋₁) = Σᵢ 𝑟ᵢ 𝑥ᵢ a result in the form of a bit
// vector (𝑟₀, …, 𝑟ₙ₋₁) corresponding to the parity function.
// Note that it is promised that the function is actually a parity
// function.
def BernsteinVazirani(qubit[nQubits] queryRegister, qubit target) -> bit[nQubits] {
    bit[nQubits] results;

    // The target qubit needs to be flipped so that a relative phase is
    // introduced when we apply a Hadamard gate and we can use
    // phase kickback when parity operation is applied.
    x target;
    h target;

    // Prepare the query register in a uniform superposition.
    PrepareUniform(queryRegister);

    // Apply the parity operation.
    ParityOperationForSecretBitstring(queryRegister, target);

    // Uncompute the preparation of the uniform superposition.
    PrepareUniform(queryRegister);

    // Measure the qubits
    results = measure queryRegister;

    // The string we are looking for is returned after execution.
    return results;
}

// Main program

// Initialize the qubits
qubit[nQubits] queryRegister;
qubit target;

reset queryRegister;
reset target;

// This register will hold and return the bit string found by the algorithm.
output bit[nQubits] results;

// Call the Bernstein-Vazirani algorithm to find the secret bit string.
results = BernsteinVazirani(queryRegister, target);
