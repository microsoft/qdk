// OpenQASM Grover's Search Algorithm
//
// Grover's search algorithm is a quantum algorithm that finds with high
// probability the unique input to a black box function that produces a
// particular output value.
//
// This program implements the Grover's algorithm for one specific function.

OPENQASM 3;
include "stdgates.inc";

// Define the number of qubits. It must be 5 for this example.
const int nQubits = 5;
// Optimal number of iterations for 5 qubits
int iterations = 4;

// Given a register in the all-zeros state, prepares a uniform
// superposition over all basis states. This is a self-adjoint operation.
def PrepareUniform(qubit[nQubits] qs) {
    for int i in [0:nQubits-1] {
        h qs[i];
    }
}

// Reflects about the basis state marked by alternating zeros and ones.
// This operation defines what input we are trying to find in the search.
def ReflectAboutMarked(qubit[nQubits] qs, qubit aux) {
    // We initialize the outputQubit to (|0⟩ - |1⟩) / √2, so that
    // toggling it results in a (-1) phase.
    x aux;
    h aux;
    // Flip the outputQubit for marked states.
    // Here, we get the state with alternating 0s and 1s by using the X
    // operation on every other qubit.
    for int i in [0:2:nQubits-1] {
        x qs[i];
    }
    // Controlled-X operation
    ctrl(nQubits) @ x qs[0], qs[1], qs[2], qs[3], qs[4], aux;

    // Undo the flips
    for int i in [0:2:nQubits-1] {
        x qs[i];
    }
    h aux;
    x aux;
}

// Function to reflect about the uniform superposition
def ReflectAboutUniform(qubit[nQubits] qs) {
    // Transform uniform superposition to all-zero
    PrepareUniform(qs);

    // Transform all-zero to all-ones
    for int i in [0:nQubits-1] {
        x qs[i];
    }

    // Reflect about all-ones
    ctrl(nQubits-1) @ z qs[0], qs[1], qs[2], qs[3], qs[4];

    // Undo transformations
    for int i in [0:nQubits-1] {
        x qs[i];
    }

    // Transform all-zero back to uniform superposition
    PrepareUniform(qs);
}

// Main program

// Allocate qubits
qubit[nQubits] qs;
qubit aux;
// The state we are looking for is returned after execution.
output bit[nQubits] results;

// Reset the qubits to the |0⟩ state before use.
reset qs;
reset aux;

// Prepare uniform superposition
PrepareUniform(qs);

for int i in [1:iterations] {
    ReflectAboutMarked(qs, aux);
    ReflectAboutUniform(qs);
}

// Measure the qubits
results = measure qs;
