// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

// angleExpression tests — direct coverage for the two helpers in `angleExpression.ts` that drive
// the Edit Argument input prompt in [contextMenu.ts](../../ux/circuit-vis/editor/contextMenu.ts):
//
//   - `isValidAngleExpression(expr)` is the predicate the prompt consults on every keystroke to
//     enable/disable OK. Anything it accepts ends up persisted into the operation's `args`.
//   - `normalizeAngleExpression(expr)` runs BEFORE validation: it trims surrounding whitespace and
//     folds case-insensitive `pi` to `π`. The persisted value is the normalized form, so OK's
//     enabled state must agree with what the user sees after Save.

// @ts-check

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  evaluateAngleExpression,
  isValidAngleExpression,
  normalizeAngleExpression,
} from "../../dist/ux/circuit-vis/angleExpression.js";

// ---------------------------------------------------------------------------
// isValidAngleExpression — positive cases
// ---------------------------------------------------------------------------

test("isValidAngleExpression: plain numbers (positive, negative, decimal) are valid", () => {
  // The simplest path — the prompt should accept any literal numeric value the user types,
  // including the "5." trailing-dot form (mirroring JavaScript's `parseFloat` tolerance).
  assert.equal(isValidAngleExpression("0"), true);
  assert.equal(isValidAngleExpression("5"), true);
  assert.equal(isValidAngleExpression("-5"), true);
  assert.equal(isValidAngleExpression("+5"), true);
  assert.equal(isValidAngleExpression("3.5"), true);
  assert.equal(isValidAngleExpression("-3.5"), true);
  assert.equal(isValidAngleExpression("5."), true);
});

test("isValidAngleExpression: bare π is valid in all four case forms", () => {
  // The prompt's placeholder example is `"π / 2.0"`; the user can type π directly via the on-screen
  // π button, or use any case variant of `pi` which `normalizeAngleExpression` folds to π before
  // this check runs.
  assert.equal(isValidAngleExpression("π"), true);
  assert.equal(isValidAngleExpression("pi"), true);
  assert.equal(isValidAngleExpression("Pi"), true);
  assert.equal(isValidAngleExpression("PI"), true);
});

test("isValidAngleExpression: signed π is valid", () => {
  // The `-π` and `+π` forms are the unary-sign-on-pi-factor path through the parser; pin both.
  assert.equal(isValidAngleExpression("-π"), true);
  assert.equal(isValidAngleExpression("+π"), true);
  assert.equal(isValidAngleExpression("-pi"), true);
});

test("isValidAngleExpression: arithmetic combinations are valid", () => {
  // The four supported binary operators, with both numbers and π. These mirror the prompt's example
  // text ("π / 2.0", "2.0 * π").
  assert.equal(isValidAngleExpression("2 + 3"), true);
  assert.equal(isValidAngleExpression("2 - 3"), true);
  assert.equal(isValidAngleExpression("2 * 3"), true);
  assert.equal(isValidAngleExpression("6 / 2"), true);
  assert.equal(isValidAngleExpression("π / 2"), true);
  assert.equal(isValidAngleExpression("2 * π"), true);
  assert.equal(isValidAngleExpression("2.0 * π"), true);
});

test("isValidAngleExpression: parentheses (including nesting) are valid", () => {
  // Recursive descent through `parseFactor`'s `lpar` branch.
  assert.equal(isValidAngleExpression("(π)"), true);
  assert.equal(isValidAngleExpression("(2 + 3) * π"), true);
  assert.equal(isValidAngleExpression("((π))"), true);
  assert.equal(isValidAngleExpression("π / (2 * 3)"), true);
});

test("isValidAngleExpression: leading/trailing whitespace is tolerated", () => {
  // `normalizeAngleExpression` (called internally by the evaluator) trims; the prompt also
  // normalizes the value BEFORE this predicate runs, so being lenient here matches what the user
  // sees after the auto-trim.
  assert.equal(isValidAngleExpression("  π  "), true);
  assert.equal(isValidAngleExpression("\tπ / 2\n"), true);
});

// ---------------------------------------------------------------------------
// isValidAngleExpression — negative cases
// ---------------------------------------------------------------------------

test("isValidAngleExpression: empty / whitespace-only input is invalid", () => {
  // The prompt's default OK-disabled state for an empty input. `evaluateAngleExpression`
  // short-circuits on a falsy `expr` or a falsy post-normalize string.
  assert.equal(isValidAngleExpression(""), false);
  assert.equal(isValidAngleExpression(" "), false);
  assert.equal(isValidAngleExpression("\t\n"), false);
});

test("isValidAngleExpression: unknown characters are invalid", () => {
  // Any character outside `[0-9.+\-*/()\sπ]` (after the `pi` → `π` fold) takes the tokenizer's
  // "unknown character" fallthrough and returns undefined. Pin a few common typos.
  assert.equal(isValidAngleExpression("π^2"), false);
  assert.equal(isValidAngleExpression("sin(π)"), false);
  assert.equal(isValidAngleExpression("π & 2"), false);
  assert.equal(isValidAngleExpression("π,2"), false);
});

test("isValidAngleExpression: malformed numbers are invalid", () => {
  // The number tokenizer requires `\d+(\.\d*)?` — no leading dot, no multiple decimals. Both are
  // rejected.
  assert.equal(
    isValidAngleExpression(".5"),
    false,
    "leading dot is not a valid number",
  );
  assert.equal(
    isValidAngleExpression("1.2.3"),
    false,
    "multiple decimal points are not a valid number",
  );
});

test("isValidAngleExpression: unbalanced parentheses are invalid", () => {
  // `parseFactor`'s `lpar` branch requires a matching `rpar` and returns undefined if it doesn't
  // see one. The "extra rpar" case fails the trailing `k !== toks.length` guard.
  assert.equal(isValidAngleExpression("(π"), false);
  assert.equal(isValidAngleExpression("π)"), false);
  assert.equal(isValidAngleExpression("((π)"), false);
});

test("isValidAngleExpression: trailing / dangling operators are invalid", () => {
  // `parseExpr` / `parseTerm` call `parseFactor` after consuming an operator; if the RHS factor is
  // missing, the parse returns undefined.
  assert.equal(isValidAngleExpression("π +"), false);
  assert.equal(isValidAngleExpression("π *"), false);
  assert.equal(isValidAngleExpression("2 +"), false);
});

test("isValidAngleExpression: lone operators / empty parens are invalid", () => {
  // No factor at all (operator only, empty parens) → parseFactor returns undefined on the first
  // call.
  assert.equal(isValidAngleExpression("+"), false);
  assert.equal(isValidAngleExpression("-"), false);
  assert.equal(isValidAngleExpression("*"), false);
  assert.equal(isValidAngleExpression("()"), false);
});

test("isValidAngleExpression: infinite results (division by zero) are invalid", () => {
  // The evaluator's final `!isFinite(result)` guard rejects `1/0` and friends — the prompt should
  // not allow the user to commit an angle that would evaluate to ±Infinity.
  assert.equal(isValidAngleExpression("1 / 0"), false);
  assert.equal(isValidAngleExpression("π / 0"), false);
  assert.equal(isValidAngleExpression("-1 / 0"), false);
});

test("isValidAngleExpression: adjacent factors without an operator are invalid", () => {
  // Implicit multiplication isn't supported; "2π" is rejected even though it's a common
  // math-notation shorthand. The parser leaves the `pi` token unconsumed after `parseFactor`
  // returns `2`, then the trailing `k !== toks.length` guard trips. (If implicit-multiply support
  // is added later, this test should be updated to expect `true`.)
  assert.equal(isValidAngleExpression("2π"), false);
  assert.equal(isValidAngleExpression("π2"), false);
});

// ---------------------------------------------------------------------------
// Generated circuit args
// ---------------------------------------------------------------------------

test("isValidAngleExpression: every form the circuit generator emits is valid", () => {
  // Circuit generation renders a rotation angle symbolically when it is a clean multiple or
  // fraction of π, and falls back to four decimals otherwise. Those strings land in the
  // operation's `args` and flow straight back into the Edit Argument prompt, so the validator has
  // to accept all of them. Keep this list in sync with the emitted forms; the explicit `*` in
  // "2 * π / 3" is load-bearing, since the implicit-multiplication test above shows "2π" would be
  // rejected.
  const emitted = [
    "0",
    "π",
    "-π",
    "3 * π",
    "-3 * π",
    "π / 4",
    "-π / 4",
    "π / 99",
    "2 * π / 3",
    "-15 * π / 16",
    "0.7854",
    "-0.3000",
  ];
  for (const expr of emitted) {
    assert.equal(isValidAngleExpression(expr), true, `rejected ${expr}`);
  }
});

test("evaluateAngleExpression: generated symbolic args evaluate to their angle", () => {
  // The state-visualization worker evaluates `args[0]` through this same helper, so a symbolic
  // arg has to produce the angle it stands for.
  const cases = [
    ["0", 0],
    ["π", Math.PI],
    ["-π", -Math.PI],
    ["3 * π", 3 * Math.PI],
    ["π / 4", Math.PI / 4],
    ["-π / 4", -Math.PI / 4],
    ["2 * π / 3", (2 * Math.PI) / 3],
  ];
  for (const [expr, expected] of cases) {
    const actual = evaluateAngleExpression(expr);
    assert.ok(
      actual !== undefined && Math.abs(actual - expected) < 1e-12,
      `${expr} evaluated to ${actual}, expected ${expected}`,
    );
  }
});

// ---------------------------------------------------------------------------
// normalizeAngleExpression
// ---------------------------------------------------------------------------

test("normalizeAngleExpression: trims surrounding whitespace", () => {
  // The prompt persists the normalized value, so leading/trailing whitespace must not survive into
  // the operation's `args`.
  assert.equal(normalizeAngleExpression("  π  "), "π");
  assert.equal(normalizeAngleExpression("\tπ / 2\n"), "π / 2");
  assert.equal(normalizeAngleExpression(""), "");
});

test("normalizeAngleExpression: folds case-insensitive 'pi' → 'π'", () => {
  // The π button on the prompt inserts the literal character, but users can also type any case
  // variant of `pi`. Both must normalize to the same persisted form so `args` doesn't depend on
  // which input path was used.
  assert.equal(normalizeAngleExpression("pi"), "π");
  assert.equal(normalizeAngleExpression("Pi"), "π");
  assert.equal(normalizeAngleExpression("PI"), "π");
  assert.equal(normalizeAngleExpression("pI"), "π");
});

test("normalizeAngleExpression: folds 'pi' embedded inside an expression", () => {
  // The fold is unanchored — every occurrence within an expression is replaced. Pin the common case
  // (a `pi` factor inside an arithmetic expression).
  assert.equal(normalizeAngleExpression("pi / 2"), "π / 2");
  assert.equal(normalizeAngleExpression("2 * Pi + PI"), "2 * π + π");
  assert.equal(normalizeAngleExpression("(pi)"), "(π)");
});

test("normalizeAngleExpression: leaves already-normalized π untouched", () => {
  // Idempotency — passing the output back through the normalizer must be a no-op. The prompt
  // re-runs the normalize step on every input event, so this property is what keeps the OK button's
  // enabled state stable.
  assert.equal(normalizeAngleExpression("π / 2"), "π / 2");
  assert.equal(normalizeAngleExpression("2 * π"), "2 * π");
  assert.equal(
    normalizeAngleExpression(normalizeAngleExpression("PI / 2")),
    "π / 2",
  );
});
