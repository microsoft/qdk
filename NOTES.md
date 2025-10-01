# Circuit diagram improvements

## Inventory of current work

- module structure - `qsc_circuit` depends on `qsc_partial_eval`, or...?
- new exports from qsc (circuit stuff)
- `operation_list_to_grid` left-alignment fixes (unrelated to the feature)
- PyQIR-based circuit builder
- A couple of RIR codegen tests (probably unnecessary)
- compute properties stored in the interpreter to support RIR generation from entrypoint
- Circuit configuration
  - circuit config object plumbed into interpreter & circuit generation
  - loop detection, generation method, group scopes, collapse registers settings
- circuit generation method options (static, eval, simulate)
- expose static circuit generation in interpreter
- update all the circuit unit tests in interpreter
- ascii art update to show boxes (in tests and Python)
- new test cases for loop, scope grouping, conditionals, custom instrinsics, variable arguments, etc
- variable arguments for gates
- source location metadata for gates
- unit tests for scoping groups specifically
- move Location to a common spot (frontend)
- partial eval changes to keep track of dbg locations, scopes and inlinedAt information
- rir changes to contain metadata (locations, scopes and instruciton metadata)
- 

## Not done, but necessary

- dashed line rendering - not showing up in light mode
- Boxes:
  - ascii art for classical wires coming out of groups is wonky
  - svg art for classical wires coming out of groups is wonky
  - figure out how to show measurement operations within boxes
  - figure out edge cases with classical and qubit controlled operations in boxes
- Fitting it into Python (`qdk` package)
- Conditionals:
  - fallback for complex conditionals
- handle maxOperations limit gracefully
- some javascript testing for diagrams

## To try out

- [x] zoom out operation grouping
- [x] block / function folding
- [x] simple conditionals
- [x] detected loops
- [ ] loops from source
- [x] qir->rir parsing (pyqir)
- [ ] qubit/argument names
- [ ] qubit/argument declaration source links
- [x] LLVM debug info
- [x] links to source code on the diagram itself
- [ ] symbolic arguments via debug metadata
- [ ] grouping vertically (qubit arrays)
- [ ] row wrapping
- [ ] unrestricted -> adaptive transformation (dead code elimination)
- [ ] fancy conditionals & cfg
- [ ] For simulated circuits only:
  - [ ] state annotations
  - [ ] dynamic circuits with ghost paths
- [x] fixed operation_list_to_grid
- [ ] scale to thousands/millions of qubits/operations
- [ ] multiple circuits for complex conditionals
- [ ] link to source for conditions
- [x] show control lines when unitary arg is a variable conditional on results (test: multiple_possible_float_values_in_unitary_arg)

## Demo files

teleportation: classically controlled (conditional) gates

simpleising: loops

xqpe: function grouping, loops, arguments derived from measurements
dotproduct: function grouping, loops, conditional gates

bell state: function grouping
deutsch jozsa: function grouping

grover.qasm, bernsteinVazirani.qasm - OpenQASM

also touch on:

- expanding collapsing
- source links
- generated from QIR
- openQasm
