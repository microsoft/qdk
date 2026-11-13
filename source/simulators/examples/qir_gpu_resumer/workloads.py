# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""The Q# sources of the `qir_gpu_resumer` example's timed workloads, shared
by `dump_programs.py` (the resumer's inputs) and `bench_current.py` (the
current GPU simulator's)."""


def bench_source(n, rounds):
    """n qubits; `rounds` rounds of Rx on every qubit, a CZ chain, MResetZ on
    q0 and X on q1 if One; then `ResetAll`; one output bit."""
    return f"""{{
    use qs = Qubit[{n}];
    mutable acc = Zero;
    for r in 0..{rounds - 1} {{
        for i in 0..{n - 1} {{ Rx(0.3 + 0.01 * Std.Convert.IntAsDouble(i + r), qs[i]); }}
        for i in 0..{n - 2} {{ CZ(qs[i], qs[i + 1]); }}
        let m = MResetZ(qs[0]);
        if m == One {{ X(qs[1]); }}
        set acc = m;
    }}
    ResetAll(qs);
    acc
}}"""


def rep_source(d, rounds):
    """A distance-d bit-flip repetition code: d data qubits, d-1 ancillas,
    `rounds` rounds of syndrome extraction, each followed by the lookup
    decoder's X corrections (feedback on the syndrome), then every data
    qubit measured. Noiseless, every syndrome is 0. The Rx(0.0) on each data
    qubit each round gives the one-qubit gates their noise."""
    lines = ["{", f"    use data = Qubit[{d}];", f"    use anc = Qubit[{d - 1}];"]
    for r in range(rounds):
        for i in range(d):
            lines.append(f"    Rx(0.0, data[{i}]);")
        for i in range(d - 1):
            lines.append(f"    CNOT(data[{i}], anc[{i}]);")
            lines.append(f"    CNOT(data[{i + 1}], anc[{i}]);")
        for i in range(d - 1):
            lines.append(f"    let s_{r}_{i} = MResetZ(anc[{i}]) == One;")
        lines.append(f"    if s_{r}_0 and not s_{r}_1 {{ X(data[0]); }}")
        for i in range(1, d - 1):
            lines.append(f"    if s_{r}_{i - 1} and s_{r}_{i} {{ X(data[{i}]); }}")
        lines.append(f"    if s_{r}_{d - 2} and not s_{r}_{d - 3} {{ X(data[{d - 1}]); }}")
    lines.append("    MResetEachZ(data)")
    lines.append("}")
    return "\n".join(lines)
