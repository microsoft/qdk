// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/*
Watch mode for a fast developer inner loop. Usage: "node watch.mjs"

WARNING: This is largely heuristics to optimize common developer workflows.
Always use ./build.py to ensure that all projects are built correctly before check-in.
Also run ./build.py to do any initial repo setup (npm install, copying 3rd party libs, etc.)

Once running, any changes to the source code for Rust directories listed, or for
the npm, vscode, docs, katas, samples or playground projects, should automatically recompile. Just
reload the playground page or reload the VS Code window to see the changes.

Notes:

- This builds the wasm module, npm package, VS Code extension, docs, katas, samples and runs the playground.
- It does NOT build Python packages or native binaries (currently).
- It does NOT build the Node.js wasm package (or run any of the node unit tests).
- It builds debug binaries unless the '--release' command line argument is provided.

*/

// @ts-check

import { subscribe } from "@parcel/watcher";
import { spawn, spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { copyWasmToVsCode, watchVsCode } from "./source/vscode/build.mjs";
import {
  buildPlayground,
  copyWasmToPlayground,
} from "./source/playground/build.js";

const thisDir = dirname(fileURLToPath(import.meta.url));
const qdkSrcDir = join(thisDir, "source");

// Watch the source directories directly to avoid notification noise from .git, __pycache__, node_modules, target, etc.
const coreDir = join(qdkSrcDir, "compiler");
const libsDir = join(thisDir, "library");
const vslsDir = join(qdkSrcDir, "language_service");
const wasmDir = join(qdkSrcDir, "wasm");
const npmDir = join(qdkSrcDir, "npm", "qsharp");
const katasDir = join(thisDir, "katas");
const samplesDir = join(thisDir, "samples");

const isWin = process.platform === "win32";
const npmCmd = isWin ? "npm.cmd" : "npm";

const release = process.argv.includes("--release");

function buildRust() {
  console.log(
    "Compiling the .wasm module " + (release ? "(release)" : "(debug)"),
  );

  // Dev build takes ~3-4 seconds on rebuild after some Rust changes. (Non-dev builds take ~15-20 seconds)
  // Build only web and not node targets to half time.

  const result = spawnSync(
    "python",
    [
      "build.py",
      "--wasm",
      "--web-only",
      "--no-check",
      "--no-test",
      release ? "" : "--debug",
    ],
    { cwd: thisDir, shell: true },
  );

  if (result.status !== 0) {
    console.error("Error during wasm build: ", result.stderr.toString());
    return;
  }

  console.log("wasm build done ", result.stderr.toString());

  // The below copies the .wasm file from the npm dir to VS Code and playground projects
  // They already watch the .d.ts file from the npm package, so will rebuild if it changes.
  copyWasmToVsCode();
  copyWasmToPlayground();
}

// Minor delay to ensure all changes flush to disk before starting a build if
// saving multiple files close together (e.g. a formatter running over a directory)
// or saving the same file multiple times (e.g. format-on-save)
const rustBuildDelayMs = 100;
let rustBuildPending = false;
function onRustChange() {
  if (rustBuildPending) return; // Already queued
  rustBuildPending = true;
  setTimeout(() => {
    // The build task runs sychronously, so we can clear the timeout handle and
    // run the build knowing that nothing will interleave with those operations
    if (rustBuildPending) {
      rustBuildPending = false;
      buildRust();
    }
  }, rustBuildDelayMs);
}

let katasBuildPending = false;
function onKatasAndSamplesChange() {
  if (katasBuildPending) return; // Already queued
  katasBuildPending = true;
  setTimeout(() => {
    // The build task runs sychronously, so we can clear the timeout handle and
    // run the build knowing that nothing will interleave with those operations
    if (katasBuildPending) {
      katasBuildPending = false;
      buildKatasAndSamples();
    }
  }, rustBuildDelayMs);
}

function buildKatasAndSamples() {
  console.log("Recompiling katas and samples content");

  const result = spawnSync(npmCmd, ["run", "generate"], {
    cwd: npmDir,
    shell: true,
  });

  console.log("Katas and samples recompiled!", result.stderr.toString());
}

/**
 *
 * @param {string} dir
 * @param {string} name
 */
function runWatcher(dir, name, watchTask = "tsc:watch") {
  console.log(`Spawning tsc:watch for ${name} in ${dir}`);
  const npmWatcher = spawn(npmCmd, ["run", watchTask], {
    cwd: dir,
    shell: true,
  });
  npmWatcher.stdout.on("data", (data) =>
    console.log(`tsc:watch ${name}: ${data}`),
  );
  npmWatcher.stderr.on("data", (data) =>
    console.error(`tsc:watch ${name} error: ${data}`),
  );
  npmWatcher.on("close", (code) =>
    console.log(`tsc:watch for ${name} exited with: `, code),
  );
}

// Do an initial Rust build
onRustChange();

// Then watch the Rust directories for code changes
[coreDir, libsDir, vslsDir, wasmDir].forEach((dir) =>
  subscribe(dir, onRustChange),
);

// Do an initial build of the Katas
onKatasAndSamplesChange();

// Watch the katas directories for code changes
[katasDir, samplesDir].forEach((dir) =>
  subscribe(dir, onKatasAndSamplesChange),
);

// Build the npm project in watch mode
runWatcher(npmDir, "npm");

// VSCode and playground are built by esbuild, but run the type checker in watch mode
runWatcher(join(qdkSrcDir, "vscode"), "vscode");
runWatcher(join(qdkSrcDir, "vscode"), "vscode webview", "tsc:watch:view");
runWatcher(join(qdkSrcDir, "playground"), "playground");

// Kick off watch mode builds (this will detect changes in the npm package it depends on) also
watchVsCode();
buildPlayground(true);
