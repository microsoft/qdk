// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import { copyFileSync, mkdirSync, cpSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { build, context } from "esbuild";

import { copyKatex } from "../vscode/build.mjs";

const thisDir = dirname(fileURLToPath(import.meta.url));
const libsDir = join(thisDir, "..", "..", "node_modules");

// Use minified libraries
const isRelease = process.argv.includes("--release");
const outdir = join(thisDir, "public/libs");

function getTimeStr() {
  const now = new Date();

  const hh = now.getHours().toString().padStart(2, "0");
  const mm = now.getMinutes().toString().padStart(2, "0");
  const ss = now.getSeconds().toString().padStart(2, "0");
  const mil = now.getMilliseconds().toString().padStart(3, "0");

  return `${hh}:${mm}:${ss}.${mil}`;
}

/** @type {import("esbuild").BuildOptions} */
const buildOptions = {
  entryPoints: [
    join(thisDir, "src/main.tsx"),
    join(thisDir, "src/compiler-worker.ts"),
    join(thisDir, "src/language-service-worker.ts"),
    join(thisDir, "src/kataViewer.tsx"),
  ],
  outdir,
  bundle: true,
  platform: "browser",
  target: ["es2022"],
  define: { "import.meta.url": "document.URL" },
  sourcemap: "linked",
  minify: isRelease ? true : false,
};

// Copy the relevant external libraries from node_modules into the static site files
function copyLibs() {
  let monacoBase = join(
    libsDir,
    `monaco-editor/${isRelease ? "min" : "dev"}/vs`,
  );
  let monacoDest = join(thisDir, `public/libs/monaco/vs`);

  console.log("Copying the Monaco files over from: " + monacoBase);
  mkdirSync(monacoDest, { recursive: true });
  cpSync(monacoBase, monacoDest, { recursive: true });

  copyKatex(join(thisDir, "public/libs/katex"));

  copyWasmToPlayground();
}

export function copyWasmToPlayground() {
  let qsharpWasm = join(thisDir, "..", "npm/qsharp/lib/web/qsc_wasm_bg.wasm");
  let qsharpDest = join(thisDir, `public/libs/qsharp`);

  console.log("Copying the wasm file to playground from: " + qsharpWasm);
  mkdirSync(qsharpDest, { recursive: true });
  copyFileSync(qsharpWasm, join(qsharpDest, "qsc_wasm_bg.wasm"));
}

function buildBundle() {
  console.log("Running esbuild");

  build(buildOptions).then(() => console.log(`Built bundles to ${outdir}`));
}

/**
 * @param {boolean} serve
 */
export async function buildPlayground(serve) {
  // Serve the site or build it?
  if (serve) {
    // Plugin to log start/end of build events (mostly to help VS Code problem matcher)
    /** @type {import("esbuild").Plugin} */
    const buildPlugin = {
      name: "Build Events",
      setup(build) {
        build.onStart(() =>
          console.log("Playground build started @ " + getTimeStr()),
        );
        build.onEnd(() =>
          console.log("Playground build complete @ " + getTimeStr()),
        );
      },
    };

    let ctx = await context({
      ...buildOptions,
      plugins: [buildPlugin],
      color: false,
    });
    const servedir = join(thisDir, "public");

    // See https://esbuild.github.io/api/#serve
    console.log(
      "Starting the playground on http://localhost:5555 (copy this URL to a browser to use the playground)",
    );
    await ctx.serve({
      port: 5555,
      servedir,
    });
  } else {
    copyLibs();
    buildBundle();
  }
}

const thisFilePath = resolve(fileURLToPath(import.meta.url));
if (thisFilePath === resolve(process.argv[1])) {
  const serve = process.argv.includes("--serve");
  buildPlayground(serve);
}
