// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import { copyFileSync, mkdirSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build as esbuildBuild, context } from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));
const libsDir = join(thisDir, "..", "..", "node_modules");

/** @type {import("esbuild").BuildOptions} */
const buildOptions = {
  entryPoints: [
    join(thisDir, "src", "extension.ts"),
    join(thisDir, "src", "compilerWorker.ts"),
    join(thisDir, "src", "debugger/debug-service-worker.ts"),
    join(thisDir, "src", "webview/webview.tsx"),
    join(thisDir, "src", "webview/editor.tsx"),
  ],
  outdir: join(thisDir, "out"),
  bundle: true,
  // minify: true,
  mainFields: ["browser", "module", "main"],
  external: ["vscode"],
  format: "cjs",
  platform: "browser",
  target: ["es2020"],
  sourcemap: "linked",
  //logLevel: "debug",
  define: { "import.meta.url": "undefined" },
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
          entryPoints: [workerEntry],
          bundle: true,
          write: false,
          platform: "browser",
          // Blob workers are classic scripts by default (not ESM), so emit an IIFE.
          format: "iife",
          target: buildOptions.target,
          sourcemap: false,
          define: buildOptions.define,
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

function getTimeStr() {
  const now = new Date();

  const hh = now.getHours().toString().padStart(2, "0");
  const mm = now.getMinutes().toString().padStart(2, "0");
  const ss = now.getSeconds().toString().padStart(2, "0");
  const mil = now.getMilliseconds().toString().padStart(3, "0");

  return `${hh}:${mm}:${ss}.${mil}`;
}

export function copyWasmToVsCode() {
  // Copy the wasm module into the extension directory
  let qsharpWasm = join(
    thisDir,
    "..",
    "npm",
    "qsharp",
    "lib",
    "web",
    "qsc_wasm_bg.wasm",
  );
  let qsharpDest = join(thisDir, `wasm`);

  console.log("Copying the wasm file to VS Code from: " + qsharpWasm);
  mkdirSync(qsharpDest, { recursive: true });
  copyFileSync(qsharpWasm, join(qsharpDest, "qsc_wasm_bg.wasm"));
}

/**
 *
 * @param {string} [destDir]
 */
export function copyKatex(destDir) {
  let katexBase = join(libsDir, `katex/dist`);
  let katexDest = destDir ?? join(thisDir, `out/katex`);

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

function buildBundle() {
  console.log("Running esbuild");

  esbuildBuild({
    ...buildOptions,
    plugins: [inlineStateComputeWorkerPlugin],
  }).then(() => console.log(`Built bundle to ${join(thisDir, "out")}`));
}

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
  let ctx = await context({
    ...buildOptions,
    plugins: [inlineStateComputeWorkerPlugin, buildPlugin],
    color: false,
  });

  ctx.watch();
}

export function copyNodeWasm() {
  const src = join(thisDir, "..", "npm", "qsharp", "lib", "nodejs");
  console.log("Copying Node.js WASM files from: " + src);

  // The bundled CJS files (server.js, extension.js) have TWO WASM loading paths:
  // 1. Inlined qsc_wasm.cjs (__commonJS wrapper) uses __dirname
  // 2. Dynamic require("../lib/nodejs/qsc_wasm.cjs") from createRequire pattern
  //    resolves relative to each bundle file
  // Both paths need the .wasm file, and path 2 needs the .cjs file on disk.

  // Path 1a: WASM binary next to server.js for inlined loader
  const mcpDest = join(thisDir, "out", "desktop", "mcp");
  mkdirSync(mcpDest, { recursive: true });
  copyFileSync(
    join(src, "qsc_wasm_bg.wasm"),
    join(mcpDest, "qsc_wasm_bg.wasm"),
  );

  // Path 1b: WASM binary next to desktop extension.js for inlined loader
  const desktopDest = join(thisDir, "out", "desktop");
  mkdirSync(desktopDest, { recursive: true });
  copyFileSync(
    join(src, "qsc_wasm_bg.wasm"),
    join(desktopDest, "qsc_wasm_bg.wasm"),
  );

  // Path 2a: CJS + WASM for createRequire from out/desktop/mcp/server.js
  //   "../lib/nodejs/qsc_wasm.cjs" → out/desktop/lib/nodejs/
  const libDestMcp = join(thisDir, "out", "desktop", "lib", "nodejs");
  mkdirSync(libDestMcp, { recursive: true });
  copyFileSync(join(src, "qsc_wasm.cjs"), join(libDestMcp, "qsc_wasm.cjs"));
  copyFileSync(
    join(src, "qsc_wasm_bg.wasm"),
    join(libDestMcp, "qsc_wasm_bg.wasm"),
  );

  // Path 2b: CJS + WASM for createRequire from out/desktop/extension.js
  //   "../lib/nodejs/qsc_wasm.cjs" → out/lib/nodejs/
  const libDestExt = join(thisDir, "out", "lib", "nodejs");
  mkdirSync(libDestExt, { recursive: true });
  copyFileSync(join(src, "qsc_wasm.cjs"), join(libDestExt, "qsc_wasm.cjs"));
  copyFileSync(
    join(src, "qsc_wasm_bg.wasm"),
    join(libDestExt, "qsc_wasm_bg.wasm"),
  );

  // Worker bundles: each worker also loads WASM via __dirname
  for (const workerDir of ["compiler", "debug-service", "language-service"]) {
    const dest = join(thisDir, "out", workerDir);
    mkdirSync(dest, { recursive: true });
    copyFileSync(join(src, "qsc_wasm_bg.wasm"), join(dest, "qsc_wasm_bg.wasm"));
  }
}

export async function buildDesktopExtension() {
  console.log("Building desktop extension bundle");

  const nodeWasmBanner = {
    js: 'var import_meta_url = typeof __filename !== "undefined" ? require("url").pathToFileURL(__filename).href : undefined;',
  };
  const nodeWasmDefine = {
    "import.meta.url": "import_meta_url",
  };

  await esbuildBuild({
    entryPoints: [join(thisDir, "src", "desktop", "extension.ts")],
    outfile: join(thisDir, "out", "desktop", "extension.js"),
    bundle: true,
    platform: "node",
    format: "cjs",
    target: ["es2022"],
    sourcemap: "linked",
    external: ["vscode"],
    banner: nodeWasmBanner,
    define: nodeWasmDefine,
    plugins: [inlineStateComputeWorkerPlugin],
  });

  // Build Node.js worker scripts as separate bundles.
  // The bundled extension.js resolves worker paths relative to __filename
  // (out/desktop/extension.js), e.g. "../compiler/worker-node.js" → "out/compiler/".
  const qsharpSrc = join(thisDir, "..", "npm", "qsharp", "src");
  const workers = [
    {
      entry: join(qsharpSrc, "compiler", "worker-node.ts"),
      outfile: join(thisDir, "out", "compiler", "worker-node.js"),
    },
    {
      entry: join(qsharpSrc, "debug-service", "worker-node.ts"),
      outfile: join(thisDir, "out", "debug-service", "worker-node.js"),
    },
    {
      entry: join(qsharpSrc, "language-service", "worker-node.ts"),
      outfile: join(thisDir, "out", "language-service", "worker-node.js"),
    },
  ];

  await Promise.all(
    workers.map((w) =>
      esbuildBuild({
        entryPoints: [w.entry],
        outfile: w.outfile,
        bundle: true,
        platform: "node",
        format: "cjs",
        target: ["es2022"],
        sourcemap: "linked",
        banner: nodeWasmBanner,
        define: nodeWasmDefine,
      }),
    ),
  );

  console.log("Desktop extension bundle built");
}

const thisFilePath = resolve(fileURLToPath(import.meta.url));
if (thisFilePath === resolve(process.argv[1])) {
  // This script being run directly (not imported)
  const isWatch = process.argv.includes("--watch");
  if (isWatch) {
    watchVsCode();
  } else {
    copyWasmToVsCode();
    copyKatex();
    buildBundle();
    await buildDesktopExtension();
  }
}
