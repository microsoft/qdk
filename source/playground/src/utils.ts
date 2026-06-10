// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { IPosition, IRange, IWorkspaceEdit } from "qsharp-lang";

type WorkspaceChange = [string, { range: IRange; newText: string }[]];

// Utility functions to convert source code to and from base64.
//
// The btoa function expects a string of bytes (i.e. in the range x00 - xFF), however
// as the code may contain UTF-16 code units outside this range, it needs to be converted
// first. If you just encodeUriComponent the whole string, it will blow up considerably as
// all spaces and newlines (common chars in code) become %20 and %0A (so 4 spaces and a newline
// and up being 15 chars) whereas base64 encoding those 5 chars in ASCII is just 'ICAgIAo' (i.e.
// 7 bytes, or less then half the size).
//
// The below functions simply convert the source to utf-8 bytes first (and as most source code
// is ASCII this is usually a one-to-one mapping), and then base64 encodes/decodes the utf-8
// bytes. In testing this results in an encoding about half the size of other methods.

// These classes aren't declared in the TypeScript DOM library yet
declare class CompressionStream implements GenericTransformStream {
  constructor(format: string);
  readable: ReadableStream<Uint8Array>;
  writable: WritableStream<Uint8Array>;
}

declare class DecompressionStream implements GenericTransformStream {
  constructor(format: string);
  readable: ReadableStream<Uint8Array>;
  writable: WritableStream<Uint8Array>;
}

// Without compression, we quickly end up with URLs which are too big (e.g. Shor's algorithm
// is too large by default). This does require a relatively "modern" browser however.
export async function codeToCompressedBase64(code: string) {
  // Get the string as UTF8 bytes.
  const myencoder = new TextEncoder();
  const byteBuff = myencoder.encode(code);

  // Compress the stream of bytes
  const compressor = new CompressionStream("gzip");
  const writer = compressor.writable.getWriter();
  writer.write(byteBuff);
  writer.close();

  // Read the compressed stream and turn into a byte string
  const compressedBuff = await new Response(compressor.readable).arrayBuffer();
  const compressedBytes = new Uint8Array(compressedBuff);

  // Turn the bytes into a string of bytes (needed for window.btoa to work)
  let binStr = "";
  for (const byte of compressedBytes) {
    binStr += String.fromCharCode(byte);
  }

  // Get the base64 representation for the string of bytes
  const base64String = window.btoa(binStr);
  return base64String;
}

export async function compressedBase64ToCode(base64: string) {
  // Turn the base64 string into a string of bytes
  const binStr = window.atob(base64);

  // Turn it into a byte array
  const byteArray = new Uint8Array(binStr.length);
  for (let i = 0; i < binStr.length; ++i) byteArray[i] = binStr.charCodeAt(i);

  // Decompress the bytes
  const decompressor = new DecompressionStream("gzip");
  const writer = decompressor.writable.getWriter();
  writer.write(byteArray);
  writer.close();

  // Read the decompressed stream and turn into a byte string
  const decompressedBuff = await new Response(
    decompressor.readable,
  ).arrayBuffer();

  // Decode the utf-8 bytes into a JavaScript string
  const decoder = new TextDecoder();
  const code = decoder.decode(decompressedBuff);
  return code;
}

export function monacoPositionToLsPosition(
  position: monaco.Position,
): IPosition {
  // Monaco uses 1-based indices while the language service (LSP) uses 0-based
  return { line: position.lineNumber - 1, character: position.column - 1 };
}

export function lsRangeToMonacoRange(range: IRange): monaco.IRange {
  // Monaco uses 1-based indices while the language service (LSP) uses 0-based
  return new monaco.Range(
    range.start.line + 1,
    range.start.character + 1,
    range.end.line + 1,
    range.end.character + 1,
  );
}

export function monacoRangetoLsRange(range: monaco.Range): IRange {
  return {
    start: {
      line: range.startLineNumber - 1,
      character: range.startColumn - 1,
    },
    end: { line: range.endLineNumber - 1, character: range.endColumn - 1 },
  };
}

export function lsToMonacoWorkspaceEdit(
  iWorkspaceEdit: IWorkspaceEdit,
): monaco.languages.WorkspaceEdit {
  const edits = (iWorkspaceEdit.changes as WorkspaceChange[]).flatMap(
    ([uri, edits]) => {
      return edits.map((edit) => {
        const textEdit: monaco.languages.TextEdit = {
          range: lsRangeToMonacoRange(edit.range),
          text: edit.newText,
        };
        return {
          resource: monaco.Uri.parse(uri),
          textEdit: textEdit,
        } as monaco.languages.IWorkspaceTextEdit;
      });
    },
  );
  return { edits: edits } as monaco.languages.WorkspaceEdit;
}
