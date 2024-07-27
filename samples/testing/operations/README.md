# Testing Operations in the QDK
This sample project demonstrates different approaches to testing operations in the QDK, both via Python and within Q# itself.

## Testing Methods

There are two primary ways to test operations in the QDK:

1. **Using Operation Matrix Representation:**
   - Use the `dump_operation` Python API to retrieve the operation's representation as a matrix and compare it against the expected matrix.

2. **Q# `Fact` Assertions:**
   - Use a `Fact` function in your Q# code that uses the `CheckOperationsAreEqual` operation to verify if two operations are identical up to a global phase. The `Fact` function asserts that the check returns `true`.

## Project Structure
This sample project is a multi-file Q# project that showcases both testing methods. The project structure is as follows:

- src
    - `BellState.qs`: Q# file containing the `AllBellStates` operation to be tested
    - `CustomOperation.qs`: Q# file containing the `ApplySWAP` operation to be tested
    - `OperationEquivalence.qs`: Q# file containing the `TestEquivalence` operation to be called in Python wrapper
- `qsharp.json`: Q# project manifest file, instructing compiler to include all files in `src` directory.
- `test_dump_operation.py`: Python wrapper containing tests.

## Installation
- Install the `qsharp` Python package by following the instructions mentioned [here](https://learn.microsoft.com/azure/quantum/install-overview-qdk#add-support-for-python-and-jupyter-notebooks).
- Install `pytest` Python package.

## Running the sample
Open the `samples/testing/operations` directory, and run `pytest` command.

## Reference Links:
- [Q# Testing guide](https://learn.microsoft.com/azure/quantum/user-guide/testing-debugging).
- [Getting started with the QDK](https://learn.microsoft.com/azure/quantum/install-overview-qdk)
- [Getting started with Pytest](https://docs.pytest.org/en/stable/getting-started.html)
