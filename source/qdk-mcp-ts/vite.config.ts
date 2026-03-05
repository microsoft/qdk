import { defineConfig } from "vite";
import preact from "@preact/preset-vite";
import { viteSingleFile } from "vite-plugin-singlefile";
import path from "node:path";

const INPUT = process.env.INPUT;
if (!INPUT) {
  throw new Error("INPUT environment variable is not set");
}

const isDevelopment = process.env.NODE_ENV === "development";

const qsharpUx = path.resolve(__dirname, "../npm/qsharp/ux");
const qsharpSrc = path.resolve(__dirname, "../npm/qsharp/src");

export default defineConfig({
  plugins: [preact(), viteSingleFile()],
  resolve: {
    alias: {
      "qsharp-lang/circuit-vis": path.join(qsharpUx, "circuit-vis/index.ts"),
      "qsharp-lang/circuit-group": path.join(
        qsharpSrc,
        "data-structures/legacyCircuitUpdate.ts",
      ),
      "qsharp-lang/qdk-theme.css": path.join(qsharpUx, "qdk-theme.css"),
      "qsharp-lang/qsharp-ux.css": path.join(qsharpUx, "qsharp-ux.css"),
      "qsharp-lang/qsharp-circuit.css": path.join(
        qsharpUx,
        "qsharp-circuit.css",
      ),
    },
  },
  build: {
    sourcemap: isDevelopment ? "inline" : undefined,
    cssMinify: !isDevelopment,
    minify: !isDevelopment,
    rollupOptions: {
      input: INPUT,
    },
    outDir: "dist",
    emptyOutDir: false,
  },
});
