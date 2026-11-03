---
title: QDK typed snapshot format
description: Current spike schema, envelope, exact values, canonical hashing and decoder budgets.
---

## Scope

The [schema](schema/snapshot.capnp) defines the current QDK-native circuit,
store and graph snapshot records. It is not a HUGR schema. This is an
experimental format with no legacy reader, converter or compatibility promise.
Domain crates own semantic models, conversion and verification; this crate
owns generated wire types, framing and bounded syntax/encoding primitives.

## Envelope

The ten-byte header is encoded independently of Cap'n Proto:

| Bytes | Meaning                                                                                |
| ----- | -------------------------------------------------------------------------------------- |
| 0-7   | `QDKCIRCT` for a complete circuit, `QDKSTORE` for a bare store, `QDKQHDG!` for a graph |
| 8     | `0x01` for packed Cap'n Proto, `0x28` for S-expression text                            |
| 9     | LF (`0x0a`), the only supported reserved flags value                                   |

The typed `Document` root has version 1 and a non-default document-kind union.
The root kind must match the magic. Domain schemas also start at version 1.
Unknown versions, kinds, flags and semantic union tags are errors. There is
exactly one complete document; trailing messages or non-whitespace text are
not accepted. No extension suffix or additional compression is defined.

## Typed values

The schema is authoritative for field ordinals and wire enums. Explicit type
IDs belong to this schema; runtime Rust enum discriminants, estimator IDs and
C API tags are not serialization identifiers.

Optional fields use explicit unions: zero, maximum IDs, an empty list and an
absent list remain distinguishable. Real values use `Real64.bits` or explicit
complex-component bits, preserving signed zero, infinities and NaN payloads.
IDs use fixed widths; `fragmentCount` is UInt64 and checked when converting to
the host's `usize`. Strings are exact UTF-8 without normalization.

Map/set projections use ordered unique keys. Instruction order, port rows,
declaration order, histories and the identity free-slot stack are not sorted.
The graph checkpoint preserves stable IDs and allocation state but excludes
runtime handle domains and analysis-cache identities.

## Text syntax

The root is `(qdk 1 DOCUMENT)`, where `DOCUMENT` is a named `circuit`, `store`
or `graph` record. Records contain `(field value)` pairs. The field/variant
names follow the schema's lower-camel-case names, not Serde field names.
Canonical output orders fields by their schema ordinals and emits every
defined field, including explicit optional absence. Input fields may be
reordered, but duplicates, unknown fields and missing fields are errors.

Lists are `(list VALUE...)`. Unions are `(variant VALUE)` or `(variant)` for
void cases; enum values are symbols. Booleans are `true` or `false`. Integers
use decimal syntax and are range-checked at the typed field. Strings use
quoted S-expression escaping. Comments, dotted/improper lists, quote/quasiquote,
vectors and reader extensions are not part of the format.

Finite reals use shortest round-tripping decimal syntax with a mandatory decimal
point (`5.0e-324` rather than `5e-324`), retaining `-0.0`. Non-finite values use `(real64-bits "0123456789abcdef")`, with exactly
sixteen hexadecimal digits; this form may also represent finite bits.
Canonical output selects decimal for finite values and the bit form otherwise.
There is no decimal NaN normalization.

Schema-specific domain codecs consume these syntax helpers; the syntax tree
is temporary and never serialized as a generic semantic map. Both binary and
text restoration run through the same domain checks.

Circuit block containment must be acyclic, including unused bodies. Forward
references and shared bodies are supported; repeatedly executing a while/for
body is not a containment cycle. Moment validation tracks the qubits actually
used by its instructions, not every wire in the declared circuit width.

## Canonical bytes and digests

Binary writing explicitly canonicalizes the typed document to a single
segment, then applies Cap'n Proto stream framing and packing. A normal
Cap'n Proto builder is not necessarily canonical.

Domain hashes use canonical uncompressed root-segment bytes, without segment
framing, packing, the envelope, or the snapshot digest itself. Each scope has
its own fixed versioned domain separator:

| Scope          | Separator bytes           | Projection                                                |
| -------------- | ------------------------- | --------------------------------------------------------- |
| Graph snapshot | `qdk.graph.snapshot.v1\0` | Complete graph payload, including revision and metadata   |
| Graph content  | `qdk.graph.content.v1\0`  | Graph content/checkpoint with revision zero               |
| Local proof    | `qdk.graph.match.v1\0`    | Matched stable nodes, their ports and incident edges only |

The graph wire digest contains exactly 32 SHA-256 bytes. Public certificate
strings can retain their existing hexadecimal representation. Integrity is
not authentication or a claim of executable graph validity.

## Resource budgets

Graph input is capped at 16 MiB including its envelope, independently of the
64 MiB expanded Cap'n Proto segment-data cap (8,388,608 words). Declared
segments are checked before allocation. Circuits do not inherit the graph
byte cap; their maximum packed expansion is bounded by the input length times
128 words, using the format's maximum zero-run expansion and checked arithmetic.

After expansion, binary traversal and logical-item budgets are sixteen times
the actual decoded word count. Each materialized list element/record must be
charged, even when its wire representation is default or zero-sized. Repeated
pointer traversal is counted by Cap'n Proto. Domain adapters charge before
allocating output collections.

Nested semantic types and graph angles allow depth 128. Wire/text wrapper
nesting is capped separately at 544 (four wrappers per semantic level plus
32 root/record wrappers). The text scanner enforces wrapper bounds before
parsing; text domain materialization starts with one item credit per payload
byte. Domain adapters must check semantic depth independently.

These limits are not a total process-memory cap. Trusted compiler-side
fingerprints do not inherit external graph-input byte limits.

## Schema generation

Generated Rust is checked in. Ordinary builds need only Rust, not the C++
schema compiler. Regeneration uses the existing entry point:

```sh
cargo run -p qsc_snapshot --features capnpc --bin generate-schema
```

Current tooling: `capnp` C++ 1.5.0, `capnpc` Rust 0.26.0, `capnp` Rust 0.26.2.
Review generated changes with the schema and current-format fixtures. Never
hand-edit generated Rust or infer a promise of historical compatibility.
