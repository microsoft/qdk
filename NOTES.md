## To try out

- [x] zoom out operation grouping
- [x] block / function folding
- [x] simple conditionals
- [x] detected loops
- [ ] loops from source
- [x] qir->rir parsing (pyqir)
- [ ] qubit/argument names
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
- [ ] show control lines when unitary arg is a variable conditional on results (test: multiple_possible_float_values_in_unitary_arg)


## Demo files

teleportation: classically controlled (conditional) gates

simpleising: loops

xqpe: function grouping, loops, arguments derived from measurements
dotproduct: function grouping, loops, conditional gates

bell state: function grouping
deutsch jozsa: function grouping


also touch on: 
- expanding collapsing
- source links
- generated from QIR
