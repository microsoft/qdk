// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { log } from "../dist/log.js";
import { getLanguageService, loadWasmModule } from "../dist/node.js";

// Load the wasm module before running any tests
const wasmPath = new URL("../lib/web/qsc_wasm_bg.wasm", import.meta.url);
await loadWasmModule(readFileSync(wasmPath).buffer);

log.setLogLevel("warn");

// Minimal IProjectHost implementation for testing
const dummyHost = {
  readFile: async () => null,
  listDirectory: async () => [],
  resolvePath: async (a, b) => b,
  fetchGithub: async () => "",
  findManifestDirectory: async () => null,
};

test("devDiagnostics configuration works", async () => {
  const languageService = await getLanguageService(dummyHost);

  try {
    // Collect diagnostics events as they are raised
    const diagnosticEvents = [];
    let notify = () => {};
    languageService.addEventListener("diagnostics", (event) => {
      diagnosticEvents.push({
        uri: event.detail.uri,
        diagnostics: event.detail.diagnostics.map((diag) => ({
          code: diag.code,
        })),
      });
      notify();
    });

    // The update loop yields to the host event loop before applying updates, so how
    // many turns this takes isn't something the test can predict.
    const nextDiagnostics = () =>
      new Promise((resolve) => {
        notify = resolve;
      });

    // Enable dev diagnostics
    await languageService.updateConfiguration({
      devDiagnostics: true,
    });

    const gotDiagnostics = nextDiagnostics();

    // Update a document
    await languageService.updateDocument(
      "test.qs",
      1,
      "namespace Test { @EntryPoint() operation Main() : Unit {} }",
      "qsharp",
    );

    await gotDiagnostics;

    // Should have received diagnostic events
    assert.deepEqual(diagnosticEvents, [
      {
        diagnostics: [
          {
            code: "Qdk.Dev.DocumentStatus",
          },
        ],
        uri: "test.qs",
      },
    ]);

    // Test disabling dev diagnostics
    diagnosticEvents.length = 0;

    const gotClearedDiagnostics = nextDiagnostics();

    await languageService.updateConfiguration({
      devDiagnostics: false,
    });

    await gotClearedDiagnostics;

    // Diagnostics should be cleared
    assert.deepEqual(diagnosticEvents, [
      {
        diagnostics: [],
        uri: "test.qs",
      },
    ]);
  } finally {
    await languageService.dispose();
  }
});
