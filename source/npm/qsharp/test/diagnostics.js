// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { QdkDiagnostics } from "../dist/diagnostics.js";
import { log } from "../dist/log.js";
import { toWasmProgramConfig } from "../dist/compiler/compiler.js";
import {
  get_qir as getRawQir,
  get_qir_with_diagnostics as getRawQirWithDiagnostics,
} from "../lib/web/qsc_wasm.js";
import {
  getCompiler,
  getCompilerWorker,
  getProjectLoader,
  loadWasmModule,
} from "../dist/node.js";

const distDir = new URL("../dist/", import.meta.url);
const compilerWorkerPath = new URL("compiler/worker.js", distDir).href;

/** @type {import("../dist/compiler/compiler.js").ProgramConfig} */
const fallbackProgram = {
  sources: [
    [
      "fallback.qs",
      `
operation Count(q : Qubit) : Int { mutable n = 0; for _ in 1..3 { X(q); set n += 1; } n }
@EntryPoint() operation Main() : (Int, Result[]) {
  use q = Qubit(); let n = Count(q); use qs = Qubit[n]; let rs = MResetEachZ(qs);
  (n + (MResetZ(q) == One ? 10 | 0), rs)
}`,
    ],
  ],
  languageFeatures: [],
  profile: "adaptive",
};

test("adaptive_fallback raw WASM and direct compatibility report one coded warning", async () => {
  const compiler = await getCompiler();
  const original = console.warn;
  const originalLogLevel = log.getLogLevel();
  const messages = [];
  log.setLogLevel("off");
  console.warn = (message) => messages.push(message);
  try {
    const result = await compiler.getQirWithDiagnostics(fallbackProgram);
    assert.equal(result.warnings.length, 1);
    assert.equal(
      result.warnings[0].diagnostic.code,
      "Qdk.Qsc.Codegen.AdaptiveProfileFallback",
    );
    assert.equal(result.warnings[0].diagnostic.severity, "warning");
    assert.equal(result.warnings[0].document, "fallback.qs");
    assert.equal(messages.length, 0);
    const config = toWasmProgramConfig(fallbackProgram, "adaptive_rif");
    const rawResult = getRawQirWithDiagnostics(config);
    assert.deepEqual(rawResult, result);
    assert.equal(messages.length, 0);
    assert.equal(getRawQir(config), result.qir);
    assert.equal(messages.length, 1);
    assert.match(messages[0], /Qdk.Qsc.Codegen.AdaptiveProfileFallback/);
    assert.match(messages[0], /QIR 1\.0/);
    messages.length = 0;
    assert.equal(await compiler.getQir(fallbackProgram), result.qir);
    assert.equal(messages.length, 1);
  } finally {
    console.warn = original;
    log.setLogLevel(originalLogLevel);
  }
});

test("adaptive_fallback worker caller-side warning preserves program", async () => {
  const compiler = getCompilerWorker(compilerWorkerPath);
  const original = console.warn;
  const messages = [];
  console.warn = (message) => messages.push(message);
  try {
    const result = await compiler.getQirWithDiagnostics(fallbackProgram);
    assert.equal(messages.length, 0);
    assert.equal(result.warnings[0].diagnostic.severity, "warning");
    assert.equal(await compiler.getQir(fallbackProgram), result.qir);
    assert.equal(messages.length, 1);
    assert.match(messages[0], /Qdk.Qsc.Codegen.AdaptiveProfileFallback/);
  } finally {
    console.warn = original;
    compiler.terminate();
  }
});

test("adaptive_fallback normal success returns QIR without warnings", async () => {
  const compiler = await getCompiler();
  const program = {
    ...fallbackProgram,
    sources: /** @type {[string,string][]} */ ([
      [
        "test.qs",
        "@EntryPoint() operation Main() : Result { use q = Qubit(); MResetZ(q) }",
      ],
    ]),
  };
  const result = await compiler.getQirWithDiagnostics(program);
  assert.deepEqual(result.warnings, []);
  assert.equal(await compiler.getQir(program), result.qir);
});

test("adaptive_fallback fatal compilation still rejects with QdkDiagnostics", async () => {
  const compiler = await getCompiler();
  await assert.rejects(
    () => compiler.getQirWithDiagnostics(getInvalidQirProgramConfig()),
    QdkDiagnostics,
  );
});

test("adaptive_fallback direct RIR preview returns both forms and coded warning", async () => {
  const compiler = await getCompiler();
  const result = await compiler.getRirWithDiagnostics(fallbackProgram);
  assert.equal(result.rir.length, 2);
  assert.deepEqual(
    result.rir,
    await compiler.getRir({ ...fallbackProgram, profile: "adaptive_rif" }),
  );
  assert.notEqual(result.rir[0], result.rir[1]);
  assert.equal(result.warnings.length, 1);
  assert.equal(
    result.warnings[0].diagnostic.code,
    "Qdk.Qsc.Codegen.AdaptiveProfileFallback",
  );
  assert.equal(result.warnings[0].diagnostic.severity, "warning");
  await assert.rejects(() => compiler.getRir(fallbackProgram), QdkDiagnostics);
});

test("adaptive_fallback rejected retry preserves original diagnostics with one note", async () => {
  const compiler = await getCompiler();
  const program = {
    ...fallbackProgram,
    sources: /** @type {[string, string][]} */ ([
      [
        "negative.qs",
        "@EntryPoint() operation Main() : Result[] { use q = Qubit(); let n = MResetZ(q) == One ? 3 | 2; use qs = Qubit[n]; MResetEachZ(qs) }",
      ],
    ]),
  };
  const original = await compiler.getRir(program).then(
    () => assert.fail("strict Adaptive must reject the program"),
    (error) => {
      assert.ok(error instanceof QdkDiagnostics);
      return error.diagnostics;
    },
  );
  await assert.rejects(
    () => compiler.getQirWithDiagnostics(program),
    (error) => {
      assert.ok(error instanceof QdkDiagnostics);
      assert.equal(error.diagnostics.length, original.length);
      for (const [index, entry] of error.diagnostics.entries()) {
        const expected = original[index];
        assert.deepEqual(
          entry,
          index === 0
            ? {
                ...expected,
                diagnostic: {
                  ...expected.diagnostic,
                  message:
                    expected.diagnostic.message +
                    "\n\nAdaptive_RIF fallback was also rejected; these diagnostics describe the requested Adaptive profile.",
                },
              }
            : expected,
        );
      }
      return true;
    },
  );
});

test("adaptive_fallback worker RIR preview transports warnings without presentation", async () => {
  const compiler = getCompilerWorker(compilerWorkerPath);
  const original = console.warn;
  const messages = [];
  console.warn = (message) => messages.push(message);
  try {
    const result = await compiler.getRirWithDiagnostics(fallbackProgram);
    assert.equal(result.rir.length, 2);
    assert.equal(result.warnings.length, 1);
    assert.equal(result.warnings[0].diagnostic.severity, "warning");
    assert.equal(messages.length, 0);
  } finally {
    console.warn = original;
    compiler.terminate();
  }
});

// Load the wasm module before running any tests
const wasmPath = new URL("../lib/web/qsc_wasm_bg.wasm", import.meta.url);
await loadWasmModule(readFileSync(wasmPath).buffer);

/** @type {import("../dist/log.js").TelemetryEvent[]} */
const telemetryEvents = [];
log.setLogLevel("warn");
log.setTelemetryCollector((event) => telemetryEvents.push(event));

/**
 * @returns {import ("../dist/compiler/compiler.js").ProgramConfig}
 */
function getInvalidQirProgramConfig() {
  /** @type {[string, string][]} */
  const sources = [
    ["test.qs", `namespace Test { function Main() : Int { return 1; } }`],
  ];
  return {
    sources,
    languageFeatures: [],
    profile: "base",
  };
}

test("getQir throws QdkDiagnostics", async () => {
  const compiler = await getCompiler();
  const invalidConfig = getInvalidQirProgramConfig();
  await assert.rejects(
    () => compiler.getQir(invalidConfig),
    (err) => {
      assert(err instanceof QdkDiagnostics, "Error should be QdkDiagnostics");
      assert(err.diagnostics.length > 0, "diagnostics should not be empty");
      assert.match(err.message, /cannot use an integer value as an output/);
      return true;
    },
    "getQir should throw on invalid input",
  );
});

test("getQir throws QdkDiagnostics - worker", async () => {
  const compiler = getCompilerWorker(compilerWorkerPath);
  const invalidConfig = getInvalidQirProgramConfig();
  try {
    await assert.rejects(
      () => compiler.getQir(invalidConfig),
      (err) => {
        assert(err instanceof QdkDiagnostics, "Error should be QdkDiagnostics");
        assert(err.diagnostics.length > 0, "diagnostics should not be empty");
        assert.match(err.message, /cannot use an integer value as an output/);
        return true;
      },
      "getQir should throw on invalid input",
    );
  } finally {
    compiler.terminate();
  }
});

// Minimal IProjectHost implementation for testing
const dummyHost = {
  readFile: async () => null,
  listDirectory: async () => [],
  resolvePath: async (a, b) => b,
  fetchGithub: async () => "",
  findManifestDirectory: async () => null,
};

test("loadQSharpProject throws QdkDiagnostics", async () => {
  const loader = await getProjectLoader(dummyHost);
  await assert.rejects(
    () => loader.loadQSharpProject("/not/a/real/dir"),
    (err) => {
      assert(err instanceof QdkDiagnostics, "Error should be QdkDiagnostics");
      assert(err.diagnostics.length > 0, "diagnostics should not be empty");
      assert.match(err.message, /Failed to parse manifest/i);
      return true;
    },
    "loadQSharpProject should throw on invalid directory",
  );
  loader.dispose();
});
