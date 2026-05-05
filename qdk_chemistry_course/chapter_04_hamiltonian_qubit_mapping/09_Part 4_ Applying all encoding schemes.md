<h2 style="color:#D30982;">Part 4: Applying all encoding schemes</h2>

All five encoding variants are applied to the same autoCAS-EOS Hamiltonian. The output table shows qubit count, Pauli term count, and Schatten norm for each.

For this small, highly symmetric system (N₂, 4 orbitals, 8 qubits) the Pauli term counts are identical across encodings. For larger, less symmetric molecules the counts diverge — BK typically reduces terms relative to JW for mid-sized systems.

The **Schatten norm** (sum of |Pauli coefficients|) is encoding-independent: it is a property of the Hamiltonian, not its representation. It can be used to set the maximum useful evolution time in QPE. The Schatten (1-)norm of the qubit Hamiltonian is the sum of the absolute values of all Pauli coefficients. In first-order Trotterized QPE, this norm bounds the maximum useful evolution time: T_max = π / norm (<a href="https://arxiv.org/abs/1912.08854" target="_blank">Childs et al., 2021</a>). A larger norm forces a shorter T_max — and since QPE energy resolution scales as 2π / (T · 2ⁿ), a smaller T means more phase bits or more Trotter repetitions are needed to reach a target precision.

This is the quantitative link between active space selection (Chapter 3) and QPE cost (Chapter 6): a larger active space produces larger Hamiltonian coefficients, a larger norm, a shorter T_max, and therefore a more demanding QPE calculation.

Before filling in the missing encoding name in the cell below, think through how the encoding affects the number of Pauli terms. Answer the question below, and then run the cell.