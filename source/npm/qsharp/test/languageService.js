// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

import assert from "node:assert/strict";
import { test } from "node:test";
import { log } from "../dist/log.js";
import { getLanguageService } from "../dist/main.js";

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
  const languageService = getLanguageService(dummyHost);

  try {
    // Collect diagnostics events as they are raised
    const diagnosticEvents = [];
    languageService.addEventListener("diagnostics", (event) => {
      diagnosticEvents.push({
        uri: event.detail.uri,
        diagnostics: event.detail.diagnostics.map((diag) => ({
          code: diag.code,
        })),
      });
    });

    // Enable dev diagnostics
    await languageService.updateConfiguration({
      devDiagnostics: true,
    });

    // Update a document
    await languageService.updateDocument(
      "test.qs",
      1,
      "namespace Test { @EntryPoint() operation Main() : Unit {} }",
      "qsharp",
    );

    await new Promise((resolve) => setTimeout(resolve, 0));

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

    await languageService.updateConfiguration({
      devDiagnostics: false,
    });

    await new Promise((resolve) => setTimeout(resolve, 0));

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
