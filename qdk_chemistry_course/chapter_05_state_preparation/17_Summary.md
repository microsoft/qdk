<h2 style="color:#D30982;">Summary</h2>

In this chapter you:
- Listed the two state preparers (`sparse_isometry_gf2x`, `qiskit_regular_isometry`) and their settings
- Prepared the HF reference state and confirmed it produces a depth-1 X-gate circuit
- Built a `truncate_wavefunction` utility and measured how circuit depth grows with determinant count
- Compared sparse vs. regular isometry on a 10-determinant state (depth 314 vs. 337,756)
- Refined the 10-determinant trial state with PMC and measured fidelity with the full SCI state
- Exported the prepared circuit to QASM, Qiskit, Q#, and QIR formats

**Key pattern (state prep + refinement):**
```python
from qdk_chemistry._core.data import SciWavefunctionContainer
from qdk_chemistry.data import Wavefunction

# Truncate to top-N determinants
wfn_10 = truncate_wavefunction(wfn_sci, 10)

# Prepare circuit
sp = create("state_prep", "sparse_isometry_gf2x")
circ = sp.run(wfn_10)

# Inspect and export
qc = circ.get_qiskit_circuit()
print(qc.count_ops())
qasm = circ.get_qasm()
```

The `wfn_10` and `circ_10` are carried forward into Chapter 6 (QPE).
