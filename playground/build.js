// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import {copyFileSync, mkdirSync, cpSync}  from "node:fs";
import {dirname, join} from "node:path";
import {fileURLToPath} from "node:url";

import {build, context} from "esbuild";

const thisDir = dirname(fileURLToPath(import.meta.url));
const libsDir = join(thisDir, "..", "node_modules");

// Use minified libraries
const isRelease = process.argv.includes('--release');
const outfile = join(thisDir, 'public/libs/app.js');

/** @type {import("esbuild").BuildOptions} */
const buildOptions = {
    entryPoints: [join(thisDir, "src/main.ts")],
    outfile,
    bundle: true,
    target: ['es2020', 'chrome64', 'edge79', 'firefox62' ,'safari11.1'],
    define: {"import.meta.url": "document.URL"},
    sourcemap: 'linked',
    minify: isRelease ? true : false,
};

// Copy the relevant external libraries from node_modules into the static site files
function copyLibs() {
    let monacoBase = join(libsDir, `monaco-editor/${isRelease ? "min" : "dev"}/vs`);
    let monacoDest = join(thisDir, `public/libs/monaco/vs`);

    console.log("Copying the Monaco files over from: " + monacoBase);
    mkdirSync(monacoDest, { recursive: true});
    cpSync(monacoBase, monacoDest, {recursive: true});

    let mathjaxBase = join(libsDir, `mathjax/es5`);
    let mathjaxDest = join(thisDir, `public/libs/mathjax`);

    console.log("Copying the Mathjax files over from: " + mathjaxBase);
    mkdirSync(mathjaxDest, { recursive: true});
    cpSync(mathjaxBase, mathjaxDest, {recursive: true});

    let qsharpWasm = join(thisDir, "..", "npm/lib/web/qsc_wasm_bg.wasm");
    let qsharpDest = join(thisDir, `public/libs/qsharp`);

    console.log("Copying the qsharp wasm file over from: " + qsharpWasm);
    mkdirSync(qsharpDest, { recursive: true});
    copyFileSync(qsharpWasm, join(qsharpDest, 'qsc_wasm_bg.wasm'));
}

function buildBundle() {
    console.log("Running esbuild");

    build(buildOptions).then(_ => console.log(`Built bundle to ${outfile}`));
}

// Serve the site or build it?
if (process.argv.includes('--serve')) {
    let ctx = await context(buildOptions);
    const servedir = join(thisDir, "public");

    // See https://esbuild.github.io/api/#serve
    console.log("Starting the playground on http://localhost:5555");
    await ctx.serve({
        port: 5555,
        servedir: 'public'
    });
} else {
    copyLibs();
    buildBundle();
}
