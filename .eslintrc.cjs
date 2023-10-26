/* eslint-env node */
module.exports = {
  extends: ["eslint:recommended", "plugin:@typescript-eslint/recommended"],
  parser: "@typescript-eslint/parser",
  plugins: ["@typescript-eslint"],
  root: true,
  ignorePatterns: [
    "/target/",
    "/playground/public/",
    "/npm/dist/",
    "/npm/lib/",
    "/npm/src/*.generated.ts",
    "/jupyterlab/lib",
    "/jupyterlab/qsharp-jupyterlab/labextension",
    "/vscode/out/",
    "/vscode/test/out/",
  ],
  env: {
    browser: true,
    node: true,
  },
  rules: {
    "@typescript-eslint/no-explicit-any": "off",
  },
};
