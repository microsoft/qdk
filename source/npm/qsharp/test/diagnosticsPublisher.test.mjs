// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

import assert from "node:assert/strict";
import { test } from "node:test";
import { DiagnosticsPublisher } from "../dist/language-service/diagnosticsPublisher.js";

/**
 * @typedef {import("../dist/../lib/web/qsc_wasm.js").VSDiagnostic} VSDiagnostic
 */

const idleDelayMs = 300;
const maxDelayMs = 1500;

/**
 * Only `code` is ever asserted on; the rest is filler to satisfy the shape.
 * @param {string} code
 * @returns {VSDiagnostic[]}
 */
function errors(code) {
  return [
    {
      range: {
        start: { line: 0, character: 0 },
        end: { line: 0, character: 1 },
      },
      message: code,
      severity: "error",
      code,
    },
  ];
}

const anError = errors("Qsc.Parse");

/**
 * Time never advances on its own here: `schedule` only records the callback, and tests
 * fire it explicitly. That keeps every case free of real delays and races.
 */
function createHarness() {
  /** @type {{ uri: string; diagnostics: VSDiagnostic[] }[]} */
  const published = [];
  /** @type {{ callback: () => void; delayMs: number; cancelled: boolean; fired: boolean }[]} */
  const timers = [];

  const publisher = new DiagnosticsPublisher({
    publish: (uri, diagnostics) => published.push({ uri, diagnostics }),
    schedule: (callback, delayMs) => {
      const timer = { callback, delayMs, cancelled: false, fired: false };
      timers.push(timer);
      return () => {
        timer.cancelled = true;
      };
    },
    delayMs: idleDelayMs,
    maxDelayMs,
  });

  /** @param {number} delayMs */
  function live(delayMs) {
    return timers.filter(
      (t) => t.delayMs === delayMs && !t.cancelled && !t.fired,
    );
  }

  /** @param {number} delayMs */
  function fire(delayMs) {
    const pending = live(delayMs);
    assert.equal(pending.length, 1, `expected one live ${delayMs}ms timer`);
    pending[0].fired = true;
    pending[0].callback();
  }

  return { publisher, published, timers, live, fire };
}

test("non-hot document publishes immediately", () => {
  const { publisher, published, timers } = createHarness();
  publisher.setHotUri("file:///a.qs");

  publisher.receive("file:///b.qs", anError);

  assert.deepEqual(published, [{ uri: "file:///b.qs", diagnostics: anError }]);
  assert.equal(timers.length, 0);
});

test("a burst with no hot document publishes every uri immediately", () => {
  const { publisher, published, timers } = createHarness();

  publisher.receive("file:///a.qs", anError);
  publisher.receive("file:///b.qs", anError);
  publisher.receive("file:///c.qs", []);

  assert.deepEqual(
    published.map((p) => p.uri),
    ["file:///a.qs", "file:///b.qs", "file:///c.qs"],
  );
  assert.equal(timers.length, 0);
});

test("hot document with errors is withheld", () => {
  const { publisher, published, live } = createHarness();
  publisher.setHotUri("file:///a.qs");

  publisher.receive("file:///a.qs", anError);

  assert.deepEqual(published, []);
  assert.equal(live(idleDelayMs).length, 1);
  assert.equal(live(maxDelayMs).length, 1);
});

test("only the latest diagnostics for the hot document are published", () => {
  const { publisher, published, fire } = createHarness();
  publisher.setHotUri("file:///a.qs");

  publisher.receive("file:///a.qs", errors("first"));
  publisher.receive("file:///a.qs", errors("second"));
  publisher.receive("file:///a.qs", errors("third"));

  fire(idleDelayMs);

  assert.deepEqual(published, [
    { uri: "file:///a.qs", diagnostics: errors("third") },
  ]);
});

test("clearing all errors publishes immediately and drops the pending entry", () => {
  const { publisher, published, live, timers } = createHarness();
  publisher.setHotUri("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  publisher.receive("file:///a.qs", []);

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: [] }]);
  assert.equal(live(idleDelayMs).length, 0);
  assert.equal(live(maxDelayMs).length, 0);
  assert.ok(timers.every((t) => t.cancelled));
});

test("switching hot document flushes the pending entry", () => {
  const { publisher, published, live } = createHarness();
  publisher.setHotUri("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  publisher.setHotUri("file:///b.qs");

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: anError }]);
  assert.equal(live(idleDelayMs).length, 0);
  assert.equal(live(maxDelayMs).length, 0);
});

test("clearing the hot document flushes and stops debouncing", () => {
  const { publisher, published, live, timers } = createHarness();
  publisher.setHotUri("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  // Stands in for the user moving to a document the language service never publishes for.
  publisher.setHotUri(undefined);

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: anError }]);
  assert.equal(live(idleDelayMs).length, 0);
  assert.equal(live(maxDelayMs).length, 0);

  publisher.receive("file:///a.qs", anError);

  assert.deepEqual(published, [
    { uri: "file:///a.qs", diagnostics: anError },
    { uri: "file:///a.qs", diagnostics: anError },
  ]);
  assert.equal(timers.length, 2);
});

test("the cap is scheduled once per burst, not per keystroke", () => {
  const { publisher, timers } = createHarness();
  publisher.setHotUri("file:///a.qs");

  publisher.receive("file:///a.qs", anError);
  publisher.receive("file:///a.qs", anError);
  publisher.receive("file:///a.qs", anError);

  assert.equal(timers.filter((t) => t.delayMs === maxDelayMs).length, 1);
  assert.equal(timers.filter((t) => t.delayMs === idleDelayMs).length, 3);
});

test("the cap publishes and cancels the idle timer", () => {
  const { publisher, published, live, fire } = createHarness();
  publisher.setHotUri("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  fire(maxDelayMs);

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: anError }]);
  assert.equal(live(idleDelayMs).length, 0);
});

test("a cap-driven publish starts a fresh cap for the next burst", () => {
  const { publisher, published, timers, fire } = createHarness();
  publisher.setHotUri("file:///a.qs");
  publisher.receive("file:///a.qs", anError);
  fire(maxDelayMs);

  publisher.receive("file:///a.qs", errors("later"));

  assert.equal(timers.filter((t) => t.delayMs === maxDelayMs).length, 2);

  fire(maxDelayMs);

  assert.deepEqual(published, [
    { uri: "file:///a.qs", diagnostics: anError },
    { uri: "file:///a.qs", diagnostics: errors("later") },
  ]);
});

test("dispose cancels without publishing", () => {
  const { publisher, published, timers } = createHarness();
  publisher.setHotUri("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  publisher.dispose();

  assert.deepEqual(published, []);
  assert.ok(timers.every((t) => t.cancelled));
});
