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

test("a document that is not being edited publishes immediately", () => {
  const { publisher, published } = createHarness();
  publisher.noteEdit("file:///a.qs");

  publisher.receive("file:///b.qs", anError);

  assert.deepEqual(published, [{ uri: "file:///b.qs", diagnostics: anError }]);
});

test("a burst with no edits publishes every uri immediately", () => {
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

test("errors for the document being edited are withheld", () => {
  const { publisher, published, live } = createHarness();
  publisher.noteEdit("file:///a.qs");

  publisher.receive("file:///a.qs", anError);

  assert.deepEqual(published, []);
  assert.equal(live(idleDelayMs).length, 1);
  assert.equal(live(maxDelayMs).length, 1);
});

test("a result that arrives after the typing stopped publishes immediately", () => {
  const { publisher, published, fire } = createHarness();
  publisher.noteEdit("file:///a.qs");

  // Stands in for a compilation slower than the wait: the burst ends before it finishes.
  fire(idleDelayMs);
  publisher.receive("file:///a.qs", anError);

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: anError }]);
});

test("the wait restarts on each edit", () => {
  const { publisher, timers, live } = createHarness();

  publisher.noteEdit("file:///a.qs");
  publisher.noteEdit("file:///a.qs");
  publisher.noteEdit("file:///a.qs");

  assert.equal(timers.filter((t) => t.delayMs === idleDelayMs).length, 3);
  assert.equal(live(idleDelayMs).length, 1);
});

test("the cap is scheduled once per burst, not per edit", () => {
  const { publisher, timers } = createHarness();

  publisher.noteEdit("file:///a.qs");
  publisher.noteEdit("file:///a.qs");
  publisher.noteEdit("file:///a.qs");

  assert.equal(timers.filter((t) => t.delayMs === maxDelayMs).length, 1);
});

test("only the latest diagnostics for the edited document are published", () => {
  const { publisher, published, fire } = createHarness();
  publisher.noteEdit("file:///a.qs");

  publisher.receive("file:///a.qs", errors("first"));
  publisher.receive("file:///a.qs", errors("second"));
  publisher.receive("file:///a.qs", errors("third"));

  fire(idleDelayMs);

  assert.deepEqual(published, [
    { uri: "file:///a.qs", diagnostics: errors("third") },
  ]);
});

test("clearing all errors publishes immediately and drops the pending entry", () => {
  const { publisher, published, live, fire } = createHarness();
  publisher.noteEdit("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  publisher.receive("file:///a.qs", []);

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: [] }]);

  // The burst belongs to the typing, not to the pending entry, so it keeps running.
  assert.equal(live(idleDelayMs).length, 1);
  assert.equal(live(maxDelayMs).length, 1);

  fire(idleDelayMs);

  assert.equal(published.length, 1);
});

test("editing another document flushes the pending entry", () => {
  const { publisher, published } = createHarness();
  publisher.noteEdit("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  publisher.noteEdit("file:///b.qs");

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: anError }]);

  // The first document is no longer the one being edited.
  publisher.receive("file:///a.qs", errors("later"));

  assert.deepEqual(published, [
    { uri: "file:///a.qs", diagnostics: anError },
    { uri: "file:///a.qs", diagnostics: errors("later") },
  ]);
});

test("editing a document we don't publish for flushes and stops debouncing", () => {
  const { publisher, published, live } = createHarness();
  publisher.noteEdit("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  publisher.noteEdit(undefined);

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: anError }]);
  assert.equal(live(idleDelayMs).length, 0);
  assert.equal(live(maxDelayMs).length, 0);

  publisher.receive("file:///a.qs", anError);

  assert.deepEqual(published, [
    { uri: "file:///a.qs", diagnostics: anError },
    { uri: "file:///a.qs", diagnostics: anError },
  ]);
});

test("the idle timer ends the burst", () => {
  const { publisher, published, live, fire } = createHarness();
  publisher.noteEdit("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  fire(idleDelayMs);

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: anError }]);
  assert.equal(live(maxDelayMs).length, 0);

  publisher.receive("file:///a.qs", errors("later"));

  assert.deepEqual(published, [
    { uri: "file:///a.qs", diagnostics: anError },
    { uri: "file:///a.qs", diagnostics: errors("later") },
  ]);
});

test("the cap publishes without ending the burst", () => {
  const { publisher, published, live, fire } = createHarness();
  publisher.noteEdit("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  fire(maxDelayMs);

  assert.deepEqual(published, [{ uri: "file:///a.qs", diagnostics: anError }]);

  // Typing hasn't stopped, so the next result is still withheld.
  assert.equal(live(idleDelayMs).length, 1);
  publisher.receive("file:///a.qs", errors("later"));

  assert.equal(published.length, 1);
});

test("a cap-driven publish starts a fresh cap on the next edit", () => {
  const { publisher, published, timers, fire } = createHarness();
  publisher.noteEdit("file:///a.qs");
  publisher.receive("file:///a.qs", anError);
  fire(maxDelayMs);

  publisher.noteEdit("file:///a.qs");
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
  publisher.noteEdit("file:///a.qs");
  publisher.receive("file:///a.qs", anError);

  publisher.dispose();

  assert.deepEqual(published, []);
  assert.ok(timers.every((t) => t.cancelled));
});
