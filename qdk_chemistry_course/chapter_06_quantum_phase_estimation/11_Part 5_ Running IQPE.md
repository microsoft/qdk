<h2 style="color:#D30982;">Part 5: Running IQPE</h2>

IQPE runs `num_bits` sequential circuits. Iteration k applies <code>U<sup>2<sup>(n−k−1)</sup></sup></code> — the first iteration is the deepest; each subsequent one halves the depth.

`pe.run()` returns a `QpeResult` with three key fields:
- `raw_energy` — the energy selected from the phase measurement (Hartree)
- `bitstring_msb_first` — the measured binary fraction, most-significant bit first
- `branching` — alias candidates: QPE measures phase modulo 2π, so multiple energies separated by 2π/T are equally consistent with the measurement. Pass a classical reference energy (e.g., from CCSD) to resolve the correct alias when the result is ambiguous.

Fill in the executor name in the cell below, and then answer the subsequent question.