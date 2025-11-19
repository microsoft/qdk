# Circuit diagram improvements

## Inventory of current work (from PR)

### Rust

- ascii art update to show boxes (in tests and Python)
- new test cases for scope grouping
- unit tests for scoping groups specifically
- python module changes to pass circuit config
- wasm plumb circuit config

### JS

- `getCircuit` takes a config object now
- "expand until depth" functionality for renderer
- react controls for expand/collapse
- VS Code circuit configuration settings
- VS Code circuit defaults

## Not done, but necessary

- Boxes:
  - ascii art for classical wires coming out of groups is wonky
  - svg art for classical wires coming out of groups is wonky
  - figure out how to show measurement operations within boxes
  - figure out edge cases with classical and qubit controlled operations in boxes
- handle maxOperations limit gracefully
- figure out / test control/adjoint calls
- source code links - what will happen on quantum os shell??
- loops from source
