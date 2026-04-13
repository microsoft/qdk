// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import { copyFileSync, cpSync, mkdirSync, readdirSync } from "node:fs";
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
  target: ["es2020"],
  sourcemap: "linked",
};

// ── Per-platform build options ──────────────────────────────────────

/** @type {Record<string, import("esbuild").BuildOptions>} */
const platformBuildOptions = {
  ui: {
    ...commonBuildOptions,
    platform: "browser",
    outdir: join(thisDir, "out", "webview"),
    entryPoints: [
      join(thisDir, "src", "webview/webview.tsx"),
      join(thisDir, "src", "webview/editor.tsx"),
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
    external: ["vscode", "web-worker"],
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

/**
 * Copy external node dependencies into node_modules/ under the extension
 * directory so they can be resolved at runtime (e.g. when installed as a VSIX).
 */
function copyNodeExternals() {
  const nodeExternals = ["web-worker"];
  for (const pkg of nodeExternals) {
    const src = join(libsDir, pkg);
    const dest = join(thisDir, "node_modules", pkg);
    console.log(`Copying external dependency ${pkg} to ${dest}`);
    cpSync(src, dest, { recursive: true });
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
  console.log(`Built bundle to ${options.outdir}`);
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
    ],
    platform: "browser",
    outdir: join(thisDir, "out", "web"),
    plugins: [inlineStateComputeWorkerPlugin, buildPlugin],
    color: false,
  });

  ctx.watch();
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
      copyNodeExternals();

      await Promise.all([
        buildPlatform("ui"),
        buildPlatform("browser"),
        buildPlatform("node"),
        buildPlatform("node-worker"),
      ]);
    }
  }
})();
