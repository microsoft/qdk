// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));

/** @type {import("esbuild").BuildOptions} */
const buildOptions = {
  entryPoints: [
    join(thisDir, "suites", "language-service", "index.ts"),
    join(thisDir, "suites", "debugger", "index.ts"),
    join(thisDir, "suites", "empty", "index.ts"),
  ],
  outdir: join(thisDir, "out"),
  bundle: true,
  mainFields: ["browser", "module", "main"],
  external: ["vscode"],
  format: "cjs",
  platform: "browser",
  target: ["es2020"],
  sourcemap: "linked",
  //logLevel: "debug",
  define: { "import.meta.url": "undefined" },
};

console.log("Running esbuild");

build(buildOptions).then(() =>
  console.log(`Built tests to ${join(thisDir, "out")}`),
);
