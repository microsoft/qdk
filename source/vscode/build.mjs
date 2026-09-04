// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import { copyFileSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build as esbuildBuild, context } from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));
const libsDir = join(thisDir, "..", "..", "node_modules");

// ── Shared esbuild options ──────────────────────────────────────────

/** @type {import("esbuild").BuildOptions} */
const commonBuildOptions = {
  bundle: true,
  external: ["vscode"],
  format: "cjs",
  target: ["es2022"],
  sourcemap: "linked",
};

// ── Per-platform build options ──────────────────────────────────────

/** @type {Record<string, import("esbuild").BuildOptions>} */
const platformBuildOptions = {
  ui: {
    ...commonBuildOptions,
    platform: "browser",
    outbase: join(thisDir, "src"),
    outdir: join(thisDir, "out"),
    entryPoints: [
      join(thisDir, "src", "webview/webview.tsx"),
      join(thisDir, "src", "webview/editor.tsx"),
      join(thisDir, "src", "webview/bloch.tsx"),
      join(thisDir, "src", "learning/webview/webview-client.tsx"),
    ],
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("browser"),
    },
    // plugins added at build time (needs inlineStateComputeWorkerPlugin)
  },
  browser: {
    ...commonBuildOptions,
    entryPoints: [
      join(thisDir, "src", "extension.ts"),
      join(thisDir, "src", "compilerWorker.ts"),
      join(thisDir, "src", "debugger/debug-service-worker.ts"),
    ],
    platform: "browser",
    outdir: join(thisDir, "out", "browser"),
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("browser"),
    },
  },
  node: {
    ...commonBuildOptions,
    platform: "node",
    outdir: join(thisDir, "out", "node"),
    entryPoints: [join(thisDir, "src", "extension.ts")],
    external: ["vscode"],
    banner: {
      js: 'const _importMetaUrl = require("url").pathToFileURL(__filename).href;',
    },
    define: {
      "import.meta.url": "_importMetaUrl",
      __PLATFORM__: JSON.stringify("node"),
    },
  },
  "node-worker": {
    ...commonBuildOptions,
    platform: "node",
    outdir: join(thisDir, "out", "node"),
    entryPoints: [
      join(thisDir, "src", "compilerWorker.ts"),
      join(thisDir, "src", "debugger/debug-service-worker.ts"),
    ],
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("node"),
    },
  },
  renderer: {
    ...commonBuildOptions,
    external: [],
    platform: "browser",
    format: "esm",
    entryPoints: [join(thisDir, "src", "notebookRenderer", "index.ts")],
    outfile: join(thisDir, "out", "renderer", "qdkLearning.js"),
    // A notebook renderer is loaded as a single JS module — VS Code won't pick
    // up a sibling stylesheet — so CSS is bundled as text and injected at
    // activation instead of emitted as a separate file.
    loader: { ".css": "text" },
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("browser"),
    },
  },
};

// ── Inline worker plugin ────────────────────────────────────────────

/** @type {import("esbuild").Plugin} */
const inlineStateComputeWorkerPlugin = {
  name: "Inline State Compute Worker",
  setup(builder) {
    builder.onResolve({ filter: /stateComputeWorker.inline\.ts$/ }, (args) => ({
      path: join(args.resolveDir, args.path),
      namespace: "inline-state-compute-worker",
    }));

    builder.onLoad(
      { filter: /.*/, namespace: "inline-state-compute-worker" },
      async () => {
        const workerEntry = join(
          thisDir,
          "src",
          "webview",
          "stateComputeWorker.ts",
        );

        const result = await esbuildBuild({
          ...commonBuildOptions,
          entryPoints: [workerEntry],
          bundle: true,
          write: false,
          platform: "browser",
          format: "iife",
          sourcemap: false,
          logLevel: "silent",
        });

        const workerSource = result.outputFiles?.[0]?.text ?? "";
        return {
          contents: `const workerSource = ${JSON.stringify(workerSource)};\nexport default workerSource;\n`,
          loader: "ts",
        };
      },
    );
  },
};

// ── Renderer/emitter contract check ─────────────────────────────────

/**
 * Fail the build if the renderer's schema and the Python emitter have drifted.
 *
 * The payload contract is written twice — TypeScript types the renderer
 * validates against, and the dicts `_learning_output.py` builds — and nothing
 * in either type system spans that gap. Checks the values whose disagreement
 * breaks a learner: MIME type, payload kinds, schema version, and the field
 * names the renderer reads.
 */
export function checkRendererContract() {
  const schemaPath = join(thisDir, "src", "notebookRenderer", "schema.ts");
  const rendererPath = join(
    thisDir,
    "src",
    "notebookRenderer",
    "multipleChoice.ts",
  );
  const emitterPath = join(
    thisDir,
    "resources",
    "qdk-learning",
    "courses",
    "chemistry-qpe",
    "_learning_output.py",
  );

  const schema = readFileSync(schemaPath, "utf8");
  const renderer = readFileSync(rendererPath, "utf8");
  const emitter = readFileSync(emitterPath, "utf8");

  const mismatches = [];
  const required = (label, value) => {
    if (value === undefined) {
      throw new Error(`Could not read ${label} while checking the contract.`);
    }
    return value;
  };

  const tsMime = required(
    "MIME_TYPE in schema.ts",
    /^export const MIME_TYPE = "([^"]+)"/m.exec(schema)?.[1],
  );
  const pyMime = required(
    "MIME_TYPE in _learning_output.py",
    /^MIME_TYPE = "([^"]+)"/m.exec(emitter)?.[1],
  );
  if (tsMime !== pyMime) {
    mismatches.push(`MIME type differs: "${tsMime}" vs "${pyMime}".`);
  }

  // Every payload the emitter builds must name a kind the renderer handles.
  const tsKinds = [...schema.matchAll(/^\s+kind: "([a-z-]+)";/gm)].map(
    (m) => m[1],
  );
  const pyKinds = [...emitter.matchAll(/"kind": "([a-z-]+)"/g)].map(
    (m) => m[1],
  );
  const unknown = pyKinds.filter((k) => !tsKinds.includes(k));
  if (unknown.length > 0) {
    mismatches.push(
      `Python emits kinds the renderer does not handle: ${[...new Set(unknown)].join(", ")}.`,
    );
  }

  const tsVersion = required(
    "SUPPORTED_SCHEMA_VERSION",
    /const SUPPORTED_SCHEMA_VERSION = (\d+)/.exec(
      readFileSync(
        join(thisDir, "src", "notebookRenderer", "index.ts"),
        "utf8",
      ),
    )?.[1],
  );
  const pyVersion = required(
    '"schemaVersion" in _learning_output.py',
    /"schemaVersion": (\d+)/.exec(emitter)?.[1],
  );
  if (tsVersion !== pyVersion) {
    mismatches.push(
      `Schema version differs: renderer accepts ${tsVersion}, emitter writes ${pyVersion}.`,
    );
  }

  // Field names the renderer reads off a multiple-choice payload. Renaming one
  // on either side leaves the question blank rather than failing loudly — or,
  // for `multiSelect`, silently builds a radio group for a question with
  // several correct answers, which then cannot be answered at all.
  //
  // The emitter writes most fields as dict literal keys (`"prompt": ...`) but
  // sets optional ones by assignment (`payload["multiSelect"] = True`), so the
  // Python probe has to accept both spellings.
  const payloadFields = ["prompt", "options", "multiSelect"];
  const optionFields = ["id", "text", "correct", "explanation"];
  for (const field of payloadFields) {
    const inTs = new RegExp(`payload\\.${field}\\b`).test(renderer);
    const inPy = new RegExp(`"${field}"\\s*(?::|\\])`).test(emitter);
    if (inTs !== inPy) {
      mismatches.push(
        `Payload field "${field}" is ${inTs ? "read by the renderer but never written by the emitter" : "written by the emitter but never read by the renderer"}.`,
      );
    }
  }
  for (const field of optionFields) {
    const inTs = new RegExp(`option\\.${field}\\b`).test(renderer);
    const inPy = new RegExp(`"${field}"`).test(emitter);
    if (inTs !== inPy) {
      mismatches.push(
        `Option field "${field}" is ${inTs ? "read by the renderer but never written by the emitter" : "written by the emitter but never read by the renderer"}.`,
      );
    }
  }

  if (mismatches.length > 0) {
    throw new Error(
      `QDK learning renderer contract mismatch:\n  - ${mismatches.join("\n  - ")}\n` +
        `Update both ${schemaPath} and ${emitterPath} together.`,
    );
  }

  const kinds = new Set(tsKinds).size;
  console.log(
    `Renderer contract OK (v${tsVersion}, ${kinds} payload kind${kinds === 1 ? "" : "s"}, ` +
      `${payloadFields.length + optionFields.length} fields).`,
  );
}

// ── Asset copy helpers ──────────────────────────────────────────────

export function copyWasmToVsCode() {
  const qsharpWasm = join(
    thisDir,
    "..",
    "npm",
    "qsharp",
    "lib",
    "web",
    "qsc_wasm_bg.wasm",
  );
  const qsharpDest = join(thisDir, "wasm");

  console.log("Copying the wasm file to VS Code from: " + qsharpWasm);
  console.log("Destination: " + qsharpDest);
  mkdirSync(qsharpDest, { recursive: true });
  copyFileSync(qsharpWasm, join(qsharpDest, "qsc_wasm_bg.wasm"));
}

/** @param {string} [destDir] */
export function copyKatex(destDir) {
  const katexBase = join(libsDir, "katex/dist");
  const katexDest = destDir ?? join(thisDir, "out/katex");
  const fontsDir = join(katexBase, "fonts");
  const fontsOutDir = join(katexDest, "fonts");

  console.log("Copying the Katex files over from: " + katexBase);
  mkdirSync(katexDest, { recursive: true });
  mkdirSync(fontsOutDir, { recursive: true });

  // katex
  copyFileSync(
    join(katexBase, "katex.min.css"),
    join(katexDest, "katex.min.css"),
  );

  // github markdown css
  copyFileSync(
    join(libsDir, "github-markdown-css/github-markdown-light.css"),
    join(katexDest, "github-markdown-light.css"),
  );
  copyFileSync(
    join(libsDir, "github-markdown-css/github-markdown-dark.css"),
    join(katexDest, "github-markdown-dark.css"),
  );

  // highlight.js css
  copyFileSync(
    join(libsDir, "highlight.js/styles/default.css"),
    join(katexDest, "hljs-light.css"),
  );
  copyFileSync(
    join(libsDir, "highlight.js/styles/dark.css"),
    join(katexDest, "hljs-dark.css"),
  );

  // vscode codicons
  copyFileSync(
    join(libsDir, "@vscode", "codicons", "dist", "codicon.css"),
    join(katexDest, "codicon.css"),
  );
  copyFileSync(
    join(libsDir, "@vscode", "codicons", "dist", "codicon.ttf"),
    join(katexDest, "codicon.ttf"),
  );

  // katex fonts
  for (const file of readdirSync(fontsDir)) {
    if (file.endsWith(".woff2")) {
      copyFileSync(join(fontsDir, file), join(fontsOutDir, file));
    }
  }
}

// ── Build functions ─────────────────────────────────────────────────

/** @param {string} platform */
async function buildPlatform(platform) {
  const options = platformBuildOptions[platform];
  if (!options) throw new Error(`Invalid platform: ${platform}`);

  // UI build needs the inline worker plugin
  if (platform === "ui") {
    options.plugins = [inlineStateComputeWorkerPlugin];
  }

  console.log(`Running esbuild for platform: ${platform}`);
  await esbuildBuild(options);
  console.log(`Built bundle to ${options.outdir ?? options.outfile}`);
}

function getTimeStr() {
  const now = new Date();
  const hh = now.getHours().toString().padStart(2, "0");
  const mm = now.getMinutes().toString().padStart(2, "0");
  const ss = now.getSeconds().toString().padStart(2, "0");
  const mil = now.getMilliseconds().toString().padStart(3, "0");
  return `${hh}:${mm}:${ss}.${mil}`;
}

// This only watches for platform = "browser" for the sake of simplicity,
// so make sure to run a full build first to catch any errors in the node
// build before pushing code changes.
export async function watchVsCode() {
  console.log("Building vscode extension in watch mode");

  /** @type {import("esbuild").Plugin} */
  const buildPlugin = {
    name: "Build Events",
    setup(build) {
      build.onStart(() =>
        console.log("VS Code build started @ " + getTimeStr()),
      );
      build.onEnd(() =>
        console.log("VS Code build complete @ " + getTimeStr()),
      );
    },
  };

  const ctx = await context({
    ...commonBuildOptions,
    entryPoints: [
      join(thisDir, "src", "extension.ts"),
      join(thisDir, "src", "compilerWorker.ts"),
      join(thisDir, "src", "debugger/debug-service-worker.ts"),
      join(thisDir, "src", "webview/webview.tsx"),
      join(thisDir, "src", "webview/editor.tsx"),
      join(thisDir, "src", "webview/bloch.tsx"),
    ],
    platform: "browser",
    outdir: join(thisDir, "out", "browser"),
    plugins: [inlineStateComputeWorkerPlugin, buildPlugin],
    color: false,
    define: {
      "import.meta.url": "undefined",
      __PLATFORM__: JSON.stringify("browser"),
    },
  });

  // The notebook renderer is a separate bundle with its own format and CSS
  // loader, so it needs its own watcher rather than another entry point above.
  const rendererCtx = await context({
    ...platformBuildOptions.renderer,
    plugins: [buildPlugin],
    color: false,
  });

  ctx.watch();
  rendererCtx.watch();
}

(async () => {
  const thisFilePath = resolve(fileURLToPath(import.meta.url));
  if (thisFilePath === resolve(process.argv[1])) {
    const isWatch = process.argv.includes("--watch");

    if (isWatch) {
      await watchVsCode();
    } else {
      copyKatex();
      copyWasmToVsCode();
      checkRendererContract();

      await Promise.all([
        buildPlatform("ui"),
        buildPlatform("browser"),
        buildPlatform("node"),
        buildPlatform("node-worker"),
        buildPlatform("renderer"),
      ]);
    }
  }
})();
