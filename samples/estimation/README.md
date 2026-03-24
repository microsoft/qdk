# Resource Estimation

This folder contains samples that show how to use the Microsoft Quantum Resource Estimator.

* [EkeraHastadFactoring.qs](./EkeraHastadFactoring.qs): Resource estimation for integer factorization using Ekerå-Håstad algorithm (Stand-alone Q# version).
* [estimation-factoring.ipynb](./estimation-factoring.ipynb): Uses EkeraHastadFactoring.qs in a Jupyter notebook to pre-configure error budget.
* [ShorRE.qs](./ShorRE.qs): Resource estimation for integer factorization using Shor's algorithm.
* [Precalculated.qs](./Precalculated.qs): Resource estimation of physical resources required for integer factorization based on precomputed logical resource estimates (`AccountForEstimates` operation).
* [estimation-frontier-widgets.ipynb](./estimation-frontier-widgets.ipynb) (Python+Q# Jupyter notebook): Explore qubit-time resource estimate tradeoffs and compare them against various algorithms and quantum stacks.
* [estimation-hardcoded-circuit.ipynb](./estimation-hardcoded-circuit.ipynb) (Python+Q# Jupyter notebook): Resource estimation for a hardcoded Q# circuit.
* [estimation-random-circuit.ipynb](./estimation-random-circuit.ipynb) (Python+Q# Jupyter notebook): Resource estimation for a randomly generated Q# circuit.
* [df-chemistry](./df-chemistry/) (Python+Q# project): Resource estimation for double-factorized chemistry algorithm.
* [estimation-ising-2D.ipynb](./estimation-ising-2D.ipynb) (Python+Q# Jupyter notebook): Resource estimation for simulating a 2D Ising Model Hamiltonian
* [estimation-heisenberg-2D.ipynb](./estimation-heisenberg-2D.ipynb) (Python+Q# Jupyter notebook): Resource estimation for simulating a 2D Heisenberg Model Hamiltonian
* [estimation-hubbard-2D.ipynb](./estimation-hubbard-2D.ipynb) (Python+Q# Jupyter notebook): Resource estimation for simulating a 2D Hubbard Model Hamiltonian
* [Dynamics.qs](./Dynamics.qs): Resource estimation for a quantum dynamics application.
