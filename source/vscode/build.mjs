// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import { copyFileSync, cpSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build as esbuildBuild, context } from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));
const libsDir = join(thisDir, "..", "..", "node_modules");

/** @type {import("esbuild").BuildOptions} */
const commonBuildOptions = {
  entryPoints: [
    join(thisDir, "src", "extension.ts"),
    join(thisDir, "src", "compilerWorker.ts"),
    join(thisDir, "src", "debugger/debug-service-worker.ts"),
  ],
  bundle: true,
  external: ["vscode"],
  format: "cjs",
  target: ["es2020"],
  sourcemap: "linked",
  define: {
    "import.meta.url": "undefined",
    __PLATFORM__: JSON.stringify("browser"),
  },
};

/** @type {import("esbuild").Plugin} */
const inlineStateComputeWorkerPlugin = {
  name: "Inline State Compute Worker",
  setup(builder) {
    builder.onResolve({ filter: /stateComputeWorker.inline\.ts$/ }, (args) => {
      // Replace the placeholder TypeScript module with a generated module
      // that exports the bundled worker JS as a string.
      return {
        path: join(args.resolveDir, args.path),
        namespace: "inline-state-compute-worker",
      };
    });

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
          // Blob workers are classic scripts by default (not ESM), so emit an IIFE.
          format: "iife",
          sourcemap: false,
          logLevel: "silent",
        });

        const workerSource = result.outputFiles?.[0]?.text ?? "";
        const contents = `const workerSource = ${JSON.stringify(workerSource)};\nexport default workerSource;\n`;

        return {
          contents,
          loader: "ts",
        };
      },
    );
  },
};

export function copyWasmToVsCode() {
  // Copy the wasm module into the extension directory
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

/**
 *
 * @param {string} [destDir]
 */
export function copyKatex(destDir) {
  const katexBase = join(libsDir, `katex/dist`);
  const katexDest = destDir ?? join(thisDir, `out/katex`);

  console.log("Copying the Katex files over from: " + katexBase);
  mkdirSync(katexDest, { recursive: true });
  copyFileSync(
    join(katexBase, "katex.min.css"),
    join(katexDest, "katex.min.css"),
  );

  // github markdown css
  copyFileSync(
    join(libsDir, `github-markdown-css/github-markdown-light.css`),
    join(katexDest, "github-markdown-light.css"),
  );
  copyFileSync(
    join(libsDir, `github-markdown-css/github-markdown-dark.css`),
    join(katexDest, "github-markdown-dark.css"),
  );

  // highlight.js css
  copyFileSync(
    join(libsDir, `highlight.js/styles/default.css`),
    join(katexDest, "hljs-light.css"),
  );
  copyFileSync(
    join(libsDir, `highlight.js/styles/dark.css`),
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

  const fontsDir = join(katexBase, "fonts");
  const fontsOutDir = join(katexDest, "fonts");

  mkdirSync(fontsOutDir, { recursive: true });

  for (const file of readdirSync(fontsDir)) {
    if (file.endsWith(".woff2")) {
      copyFileSync(join(fontsDir, file), join(fontsOutDir, file));
    }
  }
}

/**
 * @param {boolean} [onlyUI]
 * @param {string} [platform]
 * @returns {import("esbuild").BuildOptions}
 */
function getBuildOptions(onlyUI, platform) {
  if (onlyUI) {
    return {
      ...commonBuildOptions,
      platform: "browser",
      outdir: join(thisDir, "out", "webview"),
      entryPoints: [
        join(thisDir, "src", "webview/webview.tsx"),
        join(thisDir, "src", "webview/editor.tsx"),
      ],
      plugins: [inlineStateComputeWorkerPlugin],
    };
  } else if (platform === "browser") {
    return {
      ...commonBuildOptions,
      platform: "browser",
      outdir: join(thisDir, "out", "browser"),
    };
  } else if (platform === "node") {
    return {
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
    };
  } else if (platform === "node-worker") {
    return {
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
    };
  } else {
    throw new Error(`Invalid platform: ${platform}`);
  }
}

/**
 * @param {boolean} [onlyUI]
 * @param {string} [platform]
 */
function buildBundle(onlyUI, platform) {
  console.log("Running esbuild for platform: " + platform);
  const buildOptions = getBuildOptions(onlyUI, platform);
  esbuildBuild(buildOptions)
    .catch((err) => {
      console.error("Build failed:", err);
      process.exit(1);
    })
    .then(() => console.log(`Built bundle to ${buildOptions.outdir}`));
}

function buildUI() {
  copyKatex();
  buildBundle(true, "browser");
}

/**
 * @param {string} [platform]
 */
function buildExtensionHost(platform) {
  buildBundle(false, platform);
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

function getTimeStr() {
  const now = new Date();

  const hh = now.getHours().toString().padStart(2, "0");
  const mm = now.getMinutes().toString().padStart(2, "0");
  const ss = now.getSeconds().toString().padStart(2, "0");
  const mil = now.getMilliseconds().toString().padStart(3, "0");

  return `${hh}:${mm}:${ss}.${mil}`;
}

// This only watches for platform = "browser" for the sake of simplicity, so make sure to run a full build first to catch any errors in the node build before pushing code changes
export async function watchVsCode() {
  console.log("Building vscode extension in watch mode");

  // Plugin to log start/end of build events (mostly to help VS Code problem matcher)
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

const thisFilePath = resolve(fileURLToPath(import.meta.url));
if (thisFilePath === resolve(process.argv[1])) {
  // This script being run directly (not imported)
  const isWatch = process.argv.includes("--watch");
  if (isWatch) {
    watchVsCode();
  } else {
    buildUI();
    copyWasmToVsCode();
    buildExtensionHost("browser");
    buildExtensionHost("node");
    buildExtensionHost("node-worker");
    copyNodeExternals();
  }
}
