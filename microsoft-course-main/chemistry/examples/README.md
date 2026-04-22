# QDK/Chemistry examples

This directory contains example scripts demonstrating how to use QDK/Chemistry for various quantum computing chemistry tasks.

## Version compatibility

> **Important:** The `main` branch is the active development branch and may contain changes that are incompatible with the version of `qdk-chemistry` installed via pip.
> If you installed `qdk-chemistry` from PyPI (e.g., `pip install qdk-chemistry`), checkout the `stable/major.minor` branch corresponding to your installed version to ensure the examples work correctly.
> For example, if you have `qdk-chemistry` 1.0.x installed:
>
> ```bash
> git clone https://github.com/microsoft/qdk-chemistry.git
> cd qdk-chemistry
> git checkout stable/1.0
> ```
>
> You can check your installed version with `pip show qdk-chemistry`.

## Prerequisites

The base `pip install qdk-chemistry` is sufficient for importing the library, but running the examples requires additional dependencies.
The table below summarizes which [optional extras](https://github.com/microsoft/qdk-chemistry/blob/main/INSTALL.md#optional-extras) or packages are needed for each example:

| Example | Install command |
|---------|----------------|
| `qpe_stretched_n2.ipynb` | `pip install 'qdk-chemistry[jupyter]'` |
| `state_prep_energy.ipynb` | `pip install 'qdk-chemistry[jupyter]'` |
| `factory_list.ipynb` | `pip install 'qdk-chemistry[plugins]'` |
| `interoperability/pennylane/` | `pip install pennylane` |
| `interoperability/qiskit/` | `pip install 'qdk-chemistry[qiskit-extras]'` |
| `interoperability/openFermion/` | `pip install 'qdk-chemistry[openfermion-extras]'` |
| `interoperability/rdkit/` | `pip install rdkit` |

To install everything needed for all examples at once:

```bash
python -m pip install 'qdk-chemistry[all]'
python -m pip install pennylane rdkit
```

## Standalone examples and data

- `data`: Data directory for examples
- `factory_list.ipynb`: Jupyter notebook that lists available factory methods in QDK/Chemistry along with their descriptions and settings
- `language/cpp`: C++ example programs using the QDK/Chemistry C++ API
- `language/sample_sci_workflow.py`: Python script demonstrating a sample classical workflow for selected CI quantum chemistry calculations.
- `qpe_stretched_n2.ipynb`: Jupyter notebook demonstrating multi-reference quantum chemistry state preparation and iterative quantum phase estimation
- `state_prep_energy.ipynb`: Jupyter notebook demonstrating quantum state preparation and energy calculation using quantum simulators.

## Companion datasets and assets

Additional curated datasets and benchmark materials that complement these examples are available at [microsoft/qdk-chemistry-data](https://github.com/microsoft/qdk-chemistry-data).

## Examples of interoperability with other quantum computing frameworks

### PennyLane

The [`interoperability/pennylane`](interoperability/pennylane/) directory contains example programs demonstrating interoperability between QDK/Chemistry and [PennyLane](https://pennylane.ai/), including:

- [`qpe_no_trotter.py`](interoperability/pennylane/qpe_no_trotter.py): Example of Quantum Phase Estimation (QPE) without Trotterization using PennyLane and QDK/Chemistry.

### Qiskit

The [`interoperability/qiskit`](interoperability/qiskit) directory contains example programs demonstrating interoperability between QDK/Chemistry and [Qiskit](https://qiskit.org/), including:

- [`iqpe_model_hamiltonian.py`](interoperability/qiskit/iqpe_model_hamiltonian.py): Example of Iterative Quantum Phase Estimation (IQPE) using a model Hamiltonian with Qiskit and QDK/Chemistry.
- [`iqpe_no_trotter.py`](interoperability/qiskit/iqpe_no_trotter.py): Example of Iterative Quantum Phase Estimation (IQPE) without Trotterization using Qiskit and QDK/Chemistry.
- [`iqpe_trotter.py`](interoperability/qiskit/iqpe_trotter.py): Example of Iterative Quantum Phase Estimation (IQPE) with Trotterization using Qiskit and QDK/Chemistry.

### OpenFermion

The [`interoperability/openFermion`](interoperability/openFermion/) directory contains example programs demonstrating interoperability between QDK/Chemistry and [OpenFermion](https://quantumai.google/openfermion), including:

- [`molecular_hamiltonian_jordan_wigner.py`](interoperability/openFermion/molecular_hamiltonian_jordan_wigner.py): Example of Jordan-Wigner transformation using OpenFermion and QDK/Chemistry.

### RDKit

The [`interoperability/rdkit`](interoperability/rdkit/) directory contains example programs demonstrating interoperability between QDK/Chemistry and [RDKit](https://www.rdkit.org/), including:

- [`sample_rdkit_geometry.py`](interoperability/rdkit/sample_rdkit_geometry.py): Example of obtaining geometry from RDKit and calculate a simple energy with QDK/Chemistry.
