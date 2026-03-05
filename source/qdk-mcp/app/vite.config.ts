import { resolve } from "path";
import { defineConfig } from "vite";
import { viteSingleFile } from "vite-plugin-singlefile";

export default defineConfig({
  plugins: [viteSingleFile()],
  resolve: {
    alias: {
      "qdk-circuit-vis": resolve(
        __dirname,
        "../../../source/npm/qsharp/ux/circuit-vis",
      ),
      "qdk-circuit-css": resolve(
        __dirname,
        "../../../source/npm/qsharp/ux/qsharp-circuit.css",
      ),
    },
  },
  build: {
    outDir: "../src/qdk_mcp/static",
    emptyOutDir: false,
  },
});
