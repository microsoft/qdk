// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

import { ComponentGrid, Operation } from "../../data/circuit.js";
import { CircuitModel } from "../../data/circuitModel.js";
import { Location } from "../../data/location.js";
import { Register } from "../../data/register.js";
import { findOperation, getOperationRegisters } from "../../utils.js";

/*
 * `classicalRefs.ts` — classical-register producer/consumer analysis for the Action layer.
 *
 * Measurements produce classical registers; classically-controlled ops consume them. This module
 * keeps that graph consistent across moves and deletes (document-order constraints,
 * cascade-deletes, result-index remaps). Pure grid walks over the Data layer — no DOM.
 */

/**
 * Collect the classical-register IDs produced by any measurement in `op`'s subtree (including
 * `op`). Keyed `"<qubit>:<result>"`, the pair a consumer's classical control reads.
 */
const collectInternalClassicalRegs = (op: Operation): Set<string> => {
  const set = new Set<string>();
  const walk = (o: Operation): void => {
    if (o.kind === "measurement") {
      for (const r of o.results) {
        if (r.result !== undefined) {
          set.add(`${r.qubit}:${r.result}`);
        }
      }
    }
    if (o.children) {
      for (const col of o.children) {
        for (const child of col.components) {
          walk(child);
        }
      }
    }
  };
  walk(op);
  return set;
};

/**
 * Map every classical register to the location string of the measurement that produces it
 * (`"<qubit>:<result>"` → location). Locations use the editor's hierarchical format (`"0,1"`,
 * `"0,1-2,3"`), as compared by [`Location.inEarlierColumnThan`](../../data/location.ts).
 *
 * Used by `collectExternalProducerLocations` (and indirectly the dropzone filter and
 * `moveOperation` safety net) to enforce "producer column strictly earlier than consumer." If a key
 * has multiple producers (shouldn't happen in a well-formed circuit), the last one wins.
 */
const _indexProducers = (grid: ComponentGrid): Map<string, string> => {
  const map = new Map<string, string>();
  const walk = (g: ComponentGrid, prefix: string): void => {
    g.forEach((col, ci) => {
      col.components.forEach((op, oi) => {
        const loc = prefix === "" ? `${ci},${oi}` : `${prefix}-${ci},${oi}`;
        if (op.kind === "measurement") {
          for (const r of op.results) {
            if (r.result !== undefined) {
              map.set(`${r.qubit}:${r.result}`, loc);
            }
          }
        }
        if (op.children) walk(op.children, loc);
      });
    });
  };
  walk(grid, "");
  return map;
};

/**
 * For the operation at `subtreeLocation`, return the locations of every measurement that produces a
 * classical register the subtree consumes — but only producers living OUTSIDE the subtree. Internal
 * producers travel with the consumer when the subtree moves as a unit, so they impose no
 * drop-target constraint; external producers stay put, so the consumer's new position must come
 * after them.
 *
 * Used by:
 *   - The dropzone-filter pass in
 *     [`DragController.onGateMouseDown`](../../editor/controllers/dragController.ts) to hide drop
 *     targets that would invert producer-before-consumer.
 *   - The `moveOperation` safety net (returns `null` if a producer ends up after the consumer) as
 *     defense in depth.
 *
 * Returns `[]` if the op has no external classical consumers or the subtree doesn't exist.
 * Producers whose location can't be resolved are skipped.
 */
const collectExternalProducerLocations = (
  rootGrid: ComponentGrid,
  subtreeLocation: string,
): string[] => {
  const subtree = findOperation(rootGrid, subtreeLocation);
  if (subtree == null) return [];

  // Collect internal producers (their `"qubit:result"` keys) so we can exclude them from the
  // constraint check.
  const internalProducers = collectInternalClassicalRegs(subtree);

  // Walk the subtree and collect every classical-ref's key that is NOT in the internal set.
  const externalKeys = new Set<string>();
  const collectRefs = (op: Operation): void => {
    for (const r of getOperationRegisters(op)) {
      if (r.result !== undefined) {
        const key = `${r.qubit}:${r.result}`;
        if (!internalProducers.has(key)) externalKeys.add(key);
      }
    }
    if (op.children) {
      for (const col of op.children) {
        for (const c of col.components) collectRefs(c);
      }
    }
  };
  collectRefs(subtree);
  if (externalKeys.size === 0) return [];

  // Map every measurement in the grid to its location, then look up each external key.
  const producers = _indexProducers(rootGrid);
  const locations: string[] = [];
  for (const key of externalKeys) {
    const loc = producers.get(key);
    if (loc != null) locations.push(loc);
  }
  return locations;
};

/**
 * Find every consumer OUTSIDE the subtree at `subtreeLocation` whose `.controls` classical-ref
 * reads a register produced by a measurement inside the subtree. Pairs each consumer op with its
 * location; the subtree itself is skipped (its internal consumers travel/delete with it).
 *
 * Only `.controls` count, never the derived `.targets` cache — flagging `.targets` would falsely
 * mark every enclosing group and cascade-delete unrelated siblings.
 *
 * Runs BEFORE a delete: `removeOperation` renumbers producers by key and can reuse a vacated index,
 * so a post-delete scan is untrustworthy. (`findInvalidatedConsumers` is the move path's
 * post-mutation counterpart.)
 */
const collectSubtreeConsumers = (
  rootGrid: ComponentGrid,
  subtreeLocation: string,
): { op: Operation; location: string }[] => {
  const subtree = findOperation(rootGrid, subtreeLocation);
  if (subtree == null) return [];

  const producedKeys = collectInternalClassicalRegs(subtree);
  if (producedKeys.size === 0) return [];

  const consumers: { op: Operation; location: string }[] = [];
  const walk = (g: ComponentGrid, prefix: string): void => {
    g.forEach((col, ci) => {
      col.components.forEach((op, oi) => {
        const loc = prefix === "" ? `${ci},${oi}` : `${prefix}-${ci},${oi}`;
        // Skip the subtree itself; its internal consumers travel/delete with it.
        if (op === subtree) return;
        // Logical consumption lives in `.controls` only.
        const controls = op.kind === "unitary" ? op.controls : undefined;
        if (controls) {
          for (const reg of controls) {
            if (
              reg.result !== undefined &&
              producedKeys.has(`${reg.qubit}:${reg.result}`)
            ) {
              consumers.push({ op, location: loc });
              break;
            }
          }
        }
        if (op.children) walk(op.children, loc);
      });
    });
  };
  walk(rootGrid, "");
  return consumers;
};

/**
 * Scan a committed grid for consumers left dangling by the last mutation: any op whose `.controls`
 * classical-ref reads a producer that no longer sits in a strictly-earlier column (moved to/past it
 * or removed). Pairs each consumer with its location.
 *
 * The move path's single post-mutation check; kind- and producer-count-agnostic, so one pass covers
 * a group carrying several producers. Relies on the move's token pass having already repointed
 * surviving consumers — a producer's index can be reused mid-move, so only the token-preserved link
 * is trustworthy. (Add/remove renumbers by key instead, so those paths detect BEFORE mutating via
 * `collectSubtreeConsumers`.)
 */
const findInvalidatedConsumers = (
  rootGrid: ComponentGrid,
): { op: Operation; location: string }[] => {
  const producers = _indexProducers(rootGrid);
  const invalidated: { op: Operation; location: string }[] = [];
  const walk = (g: ComponentGrid, prefix: string): void => {
    g.forEach((col, ci) => {
      col.components.forEach((op, oi) => {
        const loc = prefix === "" ? `${ci},${oi}` : `${prefix}-${ci},${oi}`;
        const controls = op.kind === "unitary" ? op.controls : undefined;
        if (controls) {
          const consumerLoc = Location.parse(loc);
          for (const reg of controls) {
            if (reg.result === undefined) continue;
            const producerLoc = producers.get(`${reg.qubit}:${reg.result}`);
            const survives =
              producerLoc != null &&
              Location.parse(producerLoc).inEarlierColumnThan(consumerLoc);
            if (!survives) {
              invalidated.push({ op, location: loc });
              break;
            }
          }
        }
        if (op.children) walk(op.children, loc);
      });
    });
  };
  walk(rootGrid, "");
  return invalidated;
};

/**
 * Walk the grid for an op matching `target` by object identity and return its hierarchical location
 * string, or `null` if not found. Used by callers (e.g. `removeOperationWithDependents`) that
 * capture an op reference BEFORE a mutation that may shift its location, then need a fresh location
 * string AFTER the mutation.
 */
const findLocationByRef = (
  grid: ComponentGrid,
  target: Operation,
): string | null => {
  const walk = (g: ComponentGrid, prefix: string): string | null => {
    for (let ci = 0; ci < g.length; ci++) {
      for (let oi = 0; oi < g[ci].components.length; oi++) {
        const op = g[ci].components[oi];
        const loc = prefix === "" ? `${ci},${oi}` : `${prefix}-${ci},${oi}`;
        if (op === target) return loc;
        if (op.children) {
          const r = walk(op.children, loc);
          if (r != null) return r;
        }
      }
    }
    return null;
  };
  return walk(grid, "");
};

const walkGrid = (
  grid: ComponentGrid,
  visit: (op: Operation) => void,
): void => {
  for (const col of grid) {
    for (const op of col.components) {
      visit(op);
      if (op.children) walkGrid(op.children, visit);
    }
  }
};

const walkOperation = (op: Operation, visit: (op: Operation) => void): void => {
  visit(op);
  if (op.children) walkGrid(op.children, visit);
};

type SubjectIdentity = "preserve" | "fork";

/**
 * Give every producer and consumer a stable negative identity for one structural edit.
 */
const encodeClassicalState = (
  grid: ComponentGrid,
  subject?: Operation,
  subjectIdentity: SubjectIdentity = "fork",
): Map<number, Register> => {
  const tokenToOriginal = new Map<number, Register>();
  const keyToToken = new Map<string, number>();
  let nextToken = -1;
  const allocate = (original?: Register): number => {
    const token = nextToken--;
    if (original != null) {
      tokenToOriginal.set(token, {
        qubit: original.qubit,
        result: original.result,
      });
    }
    return token;
  };

  // Encode every existing producer before rewriting any consumers.
  walkGrid(grid, (op) => {
    if (op.kind !== "measurement") return;
    for (const result of op.results) {
      if (result.result === undefined || result.result < 0) continue;
      const key = `${result.qubit}:${result.result}`;
      const token = allocate(result);
      keyToToken.set(key, token);
      result.result = token;
    }
  });

  // Rewrite every existing consumer with its producer's token.
  walkGrid(grid, (op) => {
    const registers =
      op.kind === "measurement" ? op.qubits : getOperationRegisters(op);
    for (const register of registers) {
      if (register.result === undefined || register.result < 0) continue;
      const token = keyToToken.get(`${register.qubit}:${register.result}`);
      if (token !== undefined) register.result = token;
    }
  });

  if (subject == null) return tokenToOriginal;

  // A moved subject preserves source tokens. An added subject forks new producer identities.
  const subjectProducerTokens = new Map<string, number>();
  walkOperation(subject, (op) => {
    if (op.kind !== "measurement") return;
    for (const result of op.results) {
      if (result.result === undefined || result.result < 0) continue;
      const key = `${result.qubit}:${result.result}`;
      const token =
        subjectIdentity === "preserve" ? keyToToken.get(key) : allocate();
      if (token === undefined) continue;
      subjectProducerTokens.set(key, token);
      result.result = token;
    }
  });

  // Internal consumers follow the subject's producer token. External consumers retain the token of
  // the producer in the circuit, or their positive address when that producer is outside the edit.
  walkOperation(subject, (op) => {
    const registers =
      op.kind === "measurement" ? op.qubits : getOperationRegisters(op);
    for (const register of registers) {
      if (register.result === undefined || register.result < 0) continue;
      const key = `${register.qubit}:${register.result}`;
      const token = subjectProducerTokens.get(key) ?? keyToToken.get(key);
      if (token !== undefined) register.result = token;
    }
  });

  return tokenToOriginal;
};

/** Assign final positional results and reconnect tokenized consumers after an edit settles. */
const decodeClassicalState = (
  model: CircuitModel,
  tokenToOriginal: Map<number, Register>,
): void => {
  const tokenToFinal = new Map<number, Register>();
  const perWireCount = new Map<number, number>();

  walkGrid(model.componentGrid, (op) => {
    if (op.kind !== "measurement") return;
    for (const result of op.results) {
      if (result.result === undefined) continue;
      const token = result.result;
      const index = perWireCount.get(result.qubit) ?? 0;
      perWireCount.set(result.qubit, index + 1);
      result.result = index;
      if (token < 0) {
        tokenToFinal.set(token, { qubit: result.qubit, result: index });
      }
    }
  });

  for (let wire = 0; wire < model.qubits.length; wire++) {
    const count = perWireCount.get(wire) ?? 0;
    model.qubits[wire].numResults = count > 0 ? count : undefined;
  }

  walkGrid(model.componentGrid, (op) => {
    const registers =
      op.kind === "measurement" ? op.qubits : getOperationRegisters(op);
    for (const register of registers) {
      if (register.result === undefined || register.result >= 0) continue;
      const destination =
        tokenToFinal.get(register.result) ??
        tokenToOriginal.get(register.result);
      if (destination !== undefined) {
        register.qubit = destination.qubit;
        register.result = destination.result;
      }
    }
  });
};

export {
  encodeClassicalState,
  decodeClassicalState,
  collectInternalClassicalRegs,
  findLocationByRef,
  collectExternalProducerLocations,
  collectSubtreeConsumers,
  findInvalidatedConsumers,
};

export type { SubjectIdentity };
