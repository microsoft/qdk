# Circuit diagram improvements

## Inventory of current work (from PR)

### Rust

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
- collapsing repetition (loop detection)
- vertical qubit grouping (given a set of qubit ids)
- unit tests for left-alignment display
- unit tests for qubit grouping
- "unsupported feature" error in circuit generation
- rir->circuit transformation
  - establish variable dependencies (phi nodes and branching)
  - expand simple branches (order blocks, group operations)
  - group operations by scope stack (callable only)
  - fill in dbg metadata in circuit object (source code links)
  - collapse qubits (not super functional right now)
  - convert basic block to operations
  - filter instruction stack to user-code only
  - getting scope & location labels from metadata (e.g. Foo, Foo@34)
  - various formatting functions, mostly for debugging
  - formatting conditionals (if a = |0> etc)
  - basic binary operations formatting (half baked support) and two-result conditionals
  - mapping variables to result dependencies, feeding into control results
- unit tests for scoping groups specifically
- move Location to a common spot (frontend)
- partial eval changes to keep track of dbg locations, scopes and inlinedAt information
- rir changes to contain metadata (locations, scopes and instruciton metadata)
- partial eval unit test updates to show rir debug metadta
- rir passes changes to accommodate new metadata field
- python module changes to pass circuit config
- wasm plumb circuit config

### JS

- `getCircuit` takes a config object now
- renderer changes to show metadata links
- "expand until depth" functionality for renderer
- react controls for expand/collaps
- VS Code circuit configuration settings
- VS Code circuit defaults
- use a specific target profile fallback when generating circuits?
- "go to location" command to haandle source links
- view column fixes (unrelated)

## Not done, but necessary

- dashed line rendering - not showing up in light mode
- Boxes:
  - ascii art for classical wires coming out of groups is wonky
  - svg art for classical wires coming out of groups is wonky
  - figure out how to show measurement operations within boxes
  - figure out edge cases with classical and qubit controlled operations in boxes
- Fitting it into Python (`qdk` package). generation method? Show examples.
- Conditionals:
  - fallback for complex conditionals
- handle maxOperations limit gracefully
- some javascript testing for diagrams
- convert spans to Location at debug metadata
- source code links - what will happen on quantum os shell??
- what do we do when Unrestricted is hardcoded as target profile?

- 


## Things to call out in the spec

- must call out that editing is out of scope, and describe why. file that under "future work" idk
- must call out difference between high-level eval and circuit based on QIR (decompositions? erasing some intrinsics?)
- call out how current UI will differ from Scott's mockups (no classical wires, etc)
- call out whether we need to change the data structure or abuse the `children` field for the time being
- how qir parsing is going to fit into this

## To try out

- [ ] supporting boxes for eval/simulated circuits, maybe partial eval and regular eval share code to keep track of stacks
- [ ] loops from source
- [ ] symbolic arguments via debug metadata
- [ ] fancy conditionals & control flow-ish multiple circuits
- [ ] grouping vertically (qubit arrays)
- [x] zoom out operation grouping
- [x] block / function folding
- [x] simple conditionals
- [x] detected loops
- [x] qir->rir parsing (pyqir)
- [ ] qubit/argument names
- [ ] qubit/argument declaration source links
- [x] LLVM debug info
- [x] links to source code on the diagram itself
- [ ] row wrapping
- [ ] unrestricted -> adaptive transformation (dead code elimination)
- [ ] For simulated circuits only:
  - [ ] state annotations
  - [ ] dynamic circuits with ghost paths
- [x] fixed operation_list_to_grid
- [ ] scale to thousands/millions of qubits/operations
- [ ] multiple circuits for complex conditionals
- [ ] link to source for conditions
- [x] show control lines when unitary arg is a variable conditional on results (test: multiple_possible_float_values_in_unitary_arg)

## Relationship to Scott's proposed UI changes

1. Schema: Eliminate `children` and replace with `component` in the data structure. I'm assuming we're talking about just a schema change here - since the UI can already represent expandable sub-operations.

   - Not required yet - since I can still use the deprecated `children` property and get the sub-operations to show up in the UI. Generating this deprecated schema is harmless, since we never actually save this output to a `.qsc` file so we don't have to worry about this schema being opened up within the Circuit Editor.
   - If we ever do want to change the schema, we simply update the circuit generation code to generate the new schema.

2. UI: Sub-operations (Slide 4)

   - This is mostly already there (assuming we're using `children`). The only thing lacking is the argument mappings (e.g. [4,5] -> [0,1]) in the corners, which I think we can do without.

3. UI: Conditionals and Loops (Slides 2 and 8)

   - Not required yet - we can show these by using the `children` property with specific labels (the same as sub-operations).
   - If we ever do want to change the schema, we simply update the circuit generation code to generate the new schema

4. UI: Results Highlighting for conditionals (Slide 3)

   - Not required yet - today, we have the ability to represent conditionals as classically-controlled operations a la qiskit.

5. UI: eliminate classical wires from the rendering

   - Not required yet - purely a UI change we can make whenever we want

6. UI: Qubit allocation/deallocation (Slide 5) - out of scope
