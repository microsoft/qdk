# Circuit representation contract

The `QuantumCircuit` representation aligns with vocabulary the QDK already owns rather than introducing a parallel one. Resource estimation models logical execution today through an instruction identifier space, an encoding marker, and a property key space. A circuit-level representation that speaks those same terms can be consumed by estimation without translation.

This contract governs instruction identity, operand addressing, encoding, and properties. It does not describe storage layout, which the dense store owns.

## Shared native surface

The [native circuit facade](../circuit_api/src/lib.rs) owns complete circuit documents, named register ranges, construction sessions, instruction views, angle presentation and diagram conversion. Python remains a sibling adapter over this Rust API, not a consumer of the C ABI. Python-specific coercion, negative indexing, exceptions and context-manager syntax stay in the binding.

The facade wraps the existing dense store and compiler engines rather than replacing them. Its JSON envelope retains schema version 2 and register metadata. Builders retain their Python parents; native session tokens validate circuit ownership and ordered completion without holding raw parent pointers.

## Instruction identity

A standard gate is identified by a compact integer drawn from the existing instruction identifier space in `qdk.qre.instruction_ids`. That space holds 94 entries banded by kind within a 16-bit range:

| Band            | Range or examples                                   |
| --------------- | --------------------------------------------------- |
| Clifford        | `H` 16, `S` 23, `CX` 25, `CZ` 27, `SWAP` 28         |
| Preparation     | `PREP_X`, `PREP_Y`, `PREP_Z` near 50                |
| Measurement     | `MEAS_Z` 258 and the surrounding measurement family |
| Non-Clifford    | `T` 1028, `CCX` 1030, rotations from 1036           |
| Generic unitary | `ONE_QUBIT_UNITARY` 1280, `TWO_QUBIT_UNITARY` 1281  |
| Encoded block   | `BLOCK_*` from 1536                                 |
| Logical         | `LATTICE_SURGERY` 4352                              |
| Fallback        | `GENERIC` 65535                                     |

The dense store keeps its own inline discriminant so that appending a standard gate costs no allocation. The store exposes a total function from that discriminant to the estimator identifier. The reverse direction is partial, because the estimator space contains instructions the circuit surface does not express.

Gates with no estimator counterpart are listed explicitly. They are not silently mapped to `GENERIC`, because a silent fallback would make an unrepresentable gate indistinguishable from a deliberately generic one.

## Custom and unitary operations

Named opaque operations and explicit unitary matrices carry a boxed payload and are excluded from the allocation-free append path by design. They map onto the existing generic identifiers rather than introducing parallel names:

- One-qubit and two-qubit unitaries use `ONE_QUBIT_UNITARY` and `TWO_QUBIT_UNITARY`.
- Clifford operations of unspecified shape use `ONE_QUBIT_CLIFFORD`, `TWO_QUBIT_CLIFFORD`, or `N_QUBIT_CLIFFORD`.
- Anything outside both sets uses `GENERIC`.

## Operand addressing

Qubits and classical bits are addressed by dense integer index within the circuit. Registers are named views over index ranges, and those ranges do not overlap. Operand lists are interned, so a repeated operand pattern costs one stored list rather than one per instruction.

The circuit-level addressing model is new. It has no counterpart in the estimator, which consumes instruction counts and depths rather than operands.

## Encoding

Whether an instruction is physical or logical is a property of the instruction, not of the qubit it acts on. The contract reuses the existing `Encoding` marker with its `PHYSICAL` and `LOGICAL` values.

A distinct logical qubit type is rejected. Logical execution in QDK is already modelled as an instruction encoding combined with a target architecture and a QEC strategy, and introducing a separate qubit type would fork that working vocabulary for no gain. Operations over encoded blocks reuse the existing `BLOCK_*` instruction family.

## Control flow and blocks

Control-flow bodies are stored as blocks in a table owned by the circuit, and control-flow operations reference them by identifier. Bodies are not nested circuits.

Three consequences make this the right shape rather than a stylistic preference. Nested circuits would each carry their own operand table, so a loop body over a repeated operand pattern would re-intern it and lose the deduplication exactly where repetition is highest. The runtime intermediate representation is already a basic-block graph with explicit jumps and branches, so lowering maps block to block instead of flattening a tree. Traversal stays iterative, so deeply nested control flow does not consume stack proportional to nesting depth.

Representable is not the same as compilable. Backwards branching is a distinct target capability, present in the Adaptive and Unrestricted profiles and absent from Base, Adaptive_RI, and Adaptive_RIF. A circuit can therefore be built and stored without regard to profile, and rejected at compilation when it needs a capability the selected profile lacks, with a diagnostic naming both the construct and the capability. The alternative, silently rewriting or refusing to construct, would either change the user's program or make the authoring surface profile-dependent.

Deciding which capabilities a circuit actually needs is a compilation-time analysis. It is not readable from the kind of construct alone, so the store records structure and leaves that determination to compilation.

## Parameters and classical variables

These are two different things and the contract keeps them apart.

A parameter is symbolic at compile time. It stands in for a rotation angle or similar value and is bound before compilation, after which no trace of it remains.

A classical variable is runtime state. It is typed, it is read and written while the program executes, and it lowers onto the runtime representation's variable, allocation, load, and store instructions. Conditions in control flow are expressions over classical variables and measurement results.

Merging the two produces exactly the loose typing this contract already rejects, because a value that is sometimes bound at compile time and sometimes read at runtime cannot be checked as either.

The separation extends to the index a for loop binds, which may be either kind, and the choice decides what the index can do. A parameter index can be used as an angle, and because a compile-time value cannot vary inside a loop the target runs itself, the loop is unrolled during compilation. A variable index can be tested in a condition, and the loop can be emitted natively. Making one index serve both roles would require an angle whose value is computed at run time, which changes both the capability a circuit demands and the difficulty of decomposing a controlled rotation.

Angles are literals, parameters, or arithmetic over those. The arithmetic is a separate expression language from the one conditions use, and deliberately so: an angle expression is resolved before compilation and never reaches the target, while a condition is evaluated while the program runs. Angle expressions are stored in an arena where a node may only reference nodes stored before it, which makes reference cycles unrepresentable and lets an expression of any depth be evaluated, rendered, and rebuilt without recursion.

Conditions are stored as expression trees for the sake of the authoring surface, and lowered to the three-address form the runtime representation uses, where each operation names its destination.

## Persisted form

A circuit is saved as JSON carrying an explicit `schema_version`. Restoring rejects any other version rather than guessing at the difference.

The saved form describes the circuit, not the data structure holding it. Operands are written as resolved index lists rather than as handles into the interning tables, so the stored bytes do not depend on the order patterns happened to be interned in, and changing the interning strategy does not invalidate stored circuits. Restoring re-interns, which also discards patterns left behind by instructions that were built and then removed.

Gates are written by name rather than by discriminant, so reordering the gate enum cannot silently change the meaning of a stored circuit. The name is the same one the API uses, so there is one vocabulary rather than two.

Save and restore is total and exact. Every circuit that can be built can be written and read back, equal on width, instruction sequence, resolved operands, angles, parameters, classical variables, expressions, block bodies, and registers. Encoding is stable, so writing a restored circuit produces the same bytes as writing the original.

Non-finite angles are written as tagged strings. JSON has no representation for NaN or infinity, and nothing prevents a caller from building a circuit that carries one, so writing them as `"NaN"`, `"Infinity"`, and `"-Infinity"` is what keeps the round trip total. Finite values are ordinary JSON numbers and keep their exact bit pattern.

Restoring applies the same structural validation as building, so a saved circuit cannot introduce one that could not have been built. It does not bound the size of its input, so a caller reading a circuit from an untrusted source is responsible for limiting how much it reads.

## Properties

Instruction and circuit properties reuse the existing integer key space in `qdk.qre.property_keys`, which defines 32 built-in keys including `DISTANCE`. Custom keys use the existing base of 1,000,000 and its helper functions.

Sharing the key space means a property attached at circuit construction is readable by estimation without a translation layer.

## Rejected patterns

Two behaviours from Qiskit are deliberately not adopted, both of which Qiskit's own documentation identifies as problems.

Loosely typed instruction parameters are a recorded source of bugs that forces type-aware special casing throughout the stack. This contract requires a typed parameter model instead.

Aliasing registers, where two registers share underlying qubits, are documented as poorly supported. This contract requires registers to be non-overlapping and rejects an overlapping definition at construction time, so the failure surfaces where the mistake is made rather than in a later pass.

## Native compiler graph producers

The compiler lowers circuits, FIR callable bodies and post-partial-evaluation RIR into the same QHDG graph vocabulary. Each producer retains the source identity and ordering information its return path needs; a graph from one producer is not automatically accepted by another producer's return path.

The FIR and RIR optimization stages use the shared graph rewrite machinery, then carry supported edits back into their original representation. QIR generation continues through the established RIR emitters. Both compiler optimization stages are opt-in and independently controlled, so ordinary compilation does not remove gates intended to characterize device noise.

## The circuit-expressible subset of the graph

A circuit lowers to the QHDG graph and a graph raises back, but only one of those directions is total. Lowering is total for the subset below. Raising is best-effort, because the graph expresses programs that have no circuit form.

Fidelity is lost only when crossing between the two representations. Saving and restoring within one representation is exact, for both the circuit snapshot and the graph snapshot. These are separate properties and the diagnostics keep them separate.

A graph raises to a circuit when every node is one of the following:

- A standard gate, a reset, or a barrier, each named by the conversion.
- An allocation or release of a qubit.
- Literal and symbolic angle values feeding gates.
- A declared classical bit, or a store of a measurement result into one.
- The circuit's global phase.
- Supported conditional regions and their interface bindings.

Everything else returns a diagnostic naming the operation rather than a generic failure, because the boundary moves while the API is experimental and a caller needs to know which side of it they landed on.

Two limits apply on the way in rather than on the way out:

- Symbolic angles can cross, but circuit compilation still requires numeric binding where the selected compiler path demands it.
- If/else can cross. Loops, switches, boxes, moments, delays and storage-only custom/unitary operations remain explicit refusals in the current circuit producer.

Graph raising does not recover named circuit registers. A graph round trip is not a substitute for the complete circuit document snapshot.

Operand order is part of the contract. A two-qubit gate applied to qubits one and zero is not the same instruction as the same gate applied to zero and one, and the round trip preserves the distinction.

The subset stated here is the subset the tests cover. The exhaustive gate test is guarded by the store's own gate count, so a gate added to the store without being added to the conversion fails the build rather than silently falling outside the subset.
