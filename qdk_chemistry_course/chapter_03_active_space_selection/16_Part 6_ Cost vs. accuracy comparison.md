<h2 style="color:#D30982;">Part 6: Cost vs. accuracy comparison</h2>

Each selector has made a different bet about which orbitals matter. Now compare them on two dimensions: orbital count and energy accuracy.

The first cell compares the number of active orbitals selected by each method — the direct output of active space selection, and the primary driver of downstream cost (Hamiltonian size, circuit depth, qubit count under any encoding). You will learn more about these downstream effects in the next chapter. The second runs exact diagonalization on the four selections that share the same MP2-localized orbital basis, and so their energies are directly comparable. This is the cost-vs-accuracy tradeoff: a smaller active space is cheaper but misses correlation energy.