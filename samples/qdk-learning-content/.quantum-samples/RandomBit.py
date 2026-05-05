"""
Sample: Random Bit Generator

Demonstrates using the qsharp Python package to run a simple
quantum random number generator and collect statistics.
"""

import qsharp

# Define a Q# operation inline
qsharp.eval(
    """
    operation RandomBit() : Result {
        use q = Qubit();
        H(q);
        let r = M(q);
        Reset(q);
        r
    }
    """
)

# Run 100 shots and tally results
results = qsharp.run("RandomBit()", shots=100)

zeros = results.count(qsharp.Result.Zero)
ones = results.count(qsharp.Result.One)

print(f"Results over 100 shots:")
print(f"  |0⟩: {zeros}")
print(f"  |1⟩: {ones}")
print(f"  Ratio: {zeros/100:.2f} / {ones/100:.2f}")
