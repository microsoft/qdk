import * as esbuild from "esbuild";

// Build server.ts
await esbuild.build({
  entryPoints: ["server.ts"],
  outdir: "dist",
  bundle: true,
  platform: "node",
  format: "esm",
  target: "node18",
  packages: "external",
  banner: { js: "" },
});

// Build main.ts → dist/index.js
await esbuild.build({
  entryPoints: ["main.ts"],
  outfile: "dist/index.js",
  bundle: true,
  platform: "node",
  format: "esm",
  target: "node18",
  packages: "external",
  external: ["./server.js"],
  banner: { js: "#!/usr/bin/env node" },
});
