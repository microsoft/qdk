---
applyTo: "**/*.{rs,ts,tsx,js,jsx,mjs,cjs,json,md,css,html,yaml,yml}"
description: "Auto-format after editing Rust or JS/TS/JSON/Markdown files. Runs cargo fmt or prettier once at the end of all edits."
---

After all edits are done, run `cargo fmt` (if any `.rs` changed) and/or `npm run prettier:fix` (if any JS/TS/JSON/MD/etc. changed) once at the end.

Run `npx --no eslint --fix --rule '{"curly": ["error", "all"]}' 'source/vscode/src/learning/**/*.{ts,js}'` to enforce preferred curly brace style.
