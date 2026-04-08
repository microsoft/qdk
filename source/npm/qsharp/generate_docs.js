// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import initWasm, { generate_docs } from "./lib/web/qsc_wasm.js";

const scriptDirPath = dirname(fileURLToPath(import.meta.url));
const docsDirPath = join(scriptDirPath, "docs");

if (!existsSync(docsDirPath)) {
  mkdirSync(docsDirPath);
}

// Initialize wasm before calling any exported functions
const wasmPath = join(scriptDirPath, "lib", "web", "qsc_wasm_bg.wasm");
const wasmBytes = readFileSync(wasmPath);
await initWasm({ module_or_path: wasmBytes });

// 'filename' will be of the format 'namespace/api.md' (except for 'toc.yaml')
// 'metadata' will be the metadata that will appear at the top of the file
// 'contents' will contain the non-metadata markdown expected

/** @type {Array<{filename: string; metadata: string; contents: string}>} */
const docs = generate_docs();

if (!docs || !docs.length) {
  throw new Error("No docs generated");
}

var today = new Date();
var dd = String(today.getDate()).padStart(2, "0");
var mm = String(today.getMonth() + 1).padStart(2, "0"); //January is 0!
var yyyy = today.getFullYear();
var today_str = mm + "/" + dd + "/" + yyyy;

docs.forEach((doc) => {
  // If the filename contains a /, then we need to create the directory
  const parts = doc.filename.split("/");
  let fullPath = "";
  switch (parts.length) {
    case 1:
      if (doc.filename !== "toc.yml" && doc.filename !== "index.md") {
        throw new Error(`Invalid filename: ${doc.filename}`);
      } else {
        fullPath = join(docsDirPath, doc.filename);
      }
      break;
    case 2: {
      // Create the directory of the first part
      const dirName = join(docsDirPath, parts[0]);
      if (!existsSync(dirName)) {
        mkdirSync(dirName);
      }
      fullPath = join(dirName, parts[1]);
      break;
    }
    default:
      throw new Error(`Invalid file path: ${doc.filename}`);
  }
  var contents = "";
  if (doc.filename === "toc.yml") {
    contents = doc.contents;
  } else {
    contents =
      doc.metadata.replace("ms.date: {TIMESTAMP}", `ms.date: ${today_str}`) +
      "\n\n" +
      doc.contents;
  }
  writeFileSync(fullPath, contents);
});

console.log("Done");
