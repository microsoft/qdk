// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/*
 * Pure helpers for validating Bloch-sphere gate-code input.
 *
 * Kept in a separate module from bloch.tsx so they can be unit-tested
 * directly under plain Node without dragging in three.js, preact, or the
 * JSON data tables that bloch.tsx imports.
 */

/**
 * The set of single-character gate codes the widget understands. Each
 * character here must correspond to a `case` arm in `BlochSphere.rotate`.
 */
export const VALID_GATE_CODES = "XYZHSsTt";

/**
 * Maximum number of gates accepted from a single untrusted input (URL
 * parameter, paste into the Run textbox, etc.). Each gate animates for
 * ~100ms so even the cap here represents ~25 seconds of replay, which is
 * already past the "this is bad UX" line. The cap exists to bound the
 * abuse case (a stale or hostile link with thousands of gates flooding
 * the animation queue), not to define the intended UX limit.
 */
export const MAX_GATE_SEQUENCE_LENGTH = 256;

const validGateSet = new Set(VALID_GATE_CODES);

/**
 * Filter a string of gate codes down to characters in `VALID_GATE_CODES`
 * and cap its length at `MAX_GATE_SEQUENCE_LENGTH`. Returns the cleaned
 * string and a flag indicating whether anything was dropped, so callers
 * can decide whether to surface a hint to the user.
 */
export function sanitizeGateSequence(input: string | undefined | null): {
  gates: string;
  modified: boolean;
} {
  if (!input) return { gates: "", modified: false };
  let filtered = "";
  for (const ch of input) {
    if (validGateSet.has(ch)) filtered += ch;
  }
  const capped =
    filtered.length > MAX_GATE_SEQUENCE_LENGTH
      ? filtered.slice(0, MAX_GATE_SEQUENCE_LENGTH)
      : filtered;
  return {
    gates: capped,
    modified: capped.length !== input.length,
  };
}
