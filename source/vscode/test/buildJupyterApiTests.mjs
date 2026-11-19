//@ts-check

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));
const outfile = join(
  thisDir,
  "out",
  "node",
  "jupyter-api-contract",
  "index.node.js",
);

await build({
  entryPoints: [
    join(thisDir, "suites", "jupyter-api-contract", "index.node.ts"),
  ],
  bundle: true,
  external: ["vscode"],
  format: "cjs",
  target: ["es2022"],
  sourcemap: "linked",
  platform: "node",
  outfile,
  banner: {
    js: 'const _importMetaUrl = require("url").pathToFileURL(__filename).href;',
  },
  define: {
    "import.meta.url": "_importMetaUrl",
  },
});

console.log(`Built Jupyter API contract tests to ${outfile}`);
