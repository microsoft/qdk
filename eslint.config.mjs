import globals from "globals";
import eslint from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
    },
  },
  {
    ignores: [
      "**/.*/",
      "source/jupyterlab/lib/",
      "source/jupyterlab/qsharp-jupyterlab/labextension/",
      "source/npm/qsharp/dist/",
      "source/npm/qsharp/lib/",
      "source/npm/qsharp/src/*.generated.ts",
      "source/playground/public/",
      "source/vscode/out/",
      "source/vscode/src/katasPanel/render.js",
      "source/vscode/src/learning/panel/render.js",
      "source/vscode/test/out/",
      "source/widgets/src/qsharp_widgets/static/",
      "target/",
    ],
  },
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
);
