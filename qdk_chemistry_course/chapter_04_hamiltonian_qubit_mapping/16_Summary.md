<h2 style="color:#D30982;">Summary</h2>

In this chapter you:
- Built classical Hamiltonians for two active spaces and compared how integral counts scale with n⁴
- Listed the three qubit encodings (JW, BK, Parity) and confirmed Jordan-Wigner is the only one supported end-to-end in the QPE pipeline today
- Applied all encoding schemes and verified they produce identical spectra
- Computed the Schatten norm and its connection to the QPE time parameter T_max

**Key pattern:**
```python
ham_constructor = create("hamiltonian_constructor")
active_ham = ham_constructor.run(wfn_eos.get_orbitals())

qubit_mapper = create("qubit_mapper", "qdk", encoding="jordan-wigner")
qubit_ham = qubit_mapper.run(active_ham)

solver = create("qubit_hamiltonian_solver", "qdk_sparse_matrix_solver")
energy, _ = solver.run(qubit_ham)
```

The `active_ham` and `qubit_ham` objects are carried forward into Chapter 5 (state preparation).