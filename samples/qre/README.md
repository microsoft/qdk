# Quantum Resource Estimation Samples

This directory contains a series of Jupyter notebooks that teach you how to use
the **Quantum Resource Estimator (QRE)** from the `qdk[qre]` Python package.  The
estimator takes a quantum program and a hardware model and returns
Pareto-optimal configurations that trade off physical qubit count against
runtime within a given error budget.

## Who is this for?

These notebooks are aimed at quantum software engineers and researchers who want
to:

- Understand the physical cost of running a quantum algorithm on various architectural assumptions.
- Compare resource requirements across different error-correction codes,
  magic-state factories, and hardware assumptions.
- Build custom hardware and protocol models and plug them into the estimator.

Familiarity with basic quantum computing concepts (qubits, gates, error
correction) is helpful but not required; each notebook is self-contained and
introduces the relevant ideas as it goes.

## Suggested reading order

| # | Notebook | What you will learn |
|---|----------|---------------------|
| 0 | [Getting Started](0_getting_started.ipynb) | End-to-end workflow: define a Q# application, choose a target architecture, run the estimator, inspect the Pareto frontier, and compare runs across different error rates. |
| 1 | [Importing Quantum Programs](1_qre_input.ipynb) | How to import programs from Q#, Cirq, QIR, and OpenQASM, and how to build a custom `Application` subclass with trace parameters that QRE explores automatically. |
| 2 | [Analysing Results](2_analysing_results.ipynb) | Deep dive into the estimation output: statistics, result properties, the instruction source graph, magic-state factories, custom table columns, and Pareto-frontier plots. |
| 3 | [Building Your Own Models](3_building_your_own_models.ipynb) | How to write custom `Architecture`, QEC, and factory models to explore hypothetical hardware, and how to compose them with built-in models. |

Start with notebook 0 to learn the core concepts and API.  Notebooks 1 and 2
can be read in either order depending on whether you need to bring in programs
from other frameworks first or want to understand the output in more detail.
Notebook 3 is the most advanced and assumes familiarity with the material in the
earlier notebooks.

## Prerequisites

Install the `qdk` Python package with the `qre` extras:

```bash
pip install qdk[qre]
```
