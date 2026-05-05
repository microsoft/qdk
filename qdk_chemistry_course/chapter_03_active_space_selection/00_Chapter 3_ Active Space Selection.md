<h1 style="color:#D30982;text-align:center;">Chapter 3: Active Space Selection</h1>

<h2 style="color:#D30982;">What you'll learn</h2>

- Why the choice of active space directly determines qubit cost and why there's no single right answer
- The full menu of QDK active space selectors: `qdk_valence`, `pyscf_avas`, `qdk_occupation`, `qdk_autocas`, and `qdk_autocas_eos`
- When each method is appropriate and how to interpret what they return
- How to compare active spaces across methods by their qubit footprint

<h2 style="color:#D30982;">The fundamental tradeoff</h2>

Active space selection determines how expensive your quantum calculation will be. Typically, every spatial orbital you include costs **two qubits** (one per spin). A 10-orbital active space needs 20 qubits; a 14-orbital space needs 28. This assumes the Jordan Wigner mapping, which you will learn more about in the next chapter! The exponential scaling of exact diagonalization means this difference is not cosmetic.

The goal is to find the **smallest active space that still captures the strongly correlated physics**. In traditional quantum chemistry, this relies on the chemist's expertise: knowledge of which orbitals are chemically relevant — bonding, antibonding, lone pairs — informed by the system's symmetry and past experience with similar molecules. This works well for well-understood systems but can miss correlations in unfamiliar territory or require significant trial and error.

The QDK provides a spectrum of selectors that reduce or eliminate this reliance on prior knowledge — ranging from methods that encode chemical knowledge explicitly to methods that derive the active space automatically from computed properties of the wavefunction:

| Selector | Basis for selection | Requires |
|---|---|---|
| `qdk_valence` | Valence shell heuristic | HF wavefunction |
| `pyscf_avas` | AO-to-MO projection overlap | HF wavefunction |
| `qdk_occupation` | Natural orbital occupation numbers | Wavefunction with 1-RDM |
| `qdk_autocas` | Single-orbital entanglement entropy | Wavefunction with 1- and 2-RDM |
| `qdk_autocas_eos` | Entropy of substitution (EOS) | Wavefunction with 1- and 2-RDM |