// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Basic OpenQASM syntax highlighting for Monaco. Monaco ships a built-in
// "qsharp" language but has no OpenQASM grammar, so we register a small Monarch
// tokenizer here. Keyword and type lists are kept in sync with the TextMate
// grammar used by the VS Code extension (vscode/syntaxes/openqasm.tmLanguage.json).

const configuration: monaco.languages.LanguageConfiguration = {
  comments: {
    lineComment: "//",
    blockComment: ["/*", "*/"],
  },
  brackets: [
    ["{", "}"],
    ["[", "]"],
    ["(", ")"],
  ],
  autoClosingPairs: [
    { open: "{", close: "}" },
    { open: "[", close: "]" },
    { open: "(", close: ")" },
    { open: '"', close: '"', notIn: ["string", "comment"] },
  ],
  surroundingPairs: [
    { open: "{", close: "}" },
    { open: "[", close: "]" },
    { open: "(", close: ")" },
    { open: '"', close: '"' },
  ],
};

const language: monaco.languages.IMonarchLanguage = {
  defaultToken: "",
  keywords: [
    "OPENQASM",
    "include",
    "gate",
    "def",
    "defcal",
    "defcalgrammar",
    "cal",
    "pragma",
    "extern",
    "box",
    "barrier",
    "delay",
    "reset",
    "measure",
    "let",
    "const",
    "input",
    "output",
    "return",
    "end",
    "if",
    "else",
    "for",
    "in",
    "while",
    "switch",
    "case",
    "default",
    "gphase",
    "ctrl",
    "negctrl",
    "inv",
    "pow",
    "durationof",
    "sizeof",
    "mutable",
    "readonly",
    "port",
    "frame",
    "waveform",
  ],
  typeKeywords: [
    "qubit",
    "bit",
    "bool",
    "int",
    "uint",
    "float",
    "angle",
    "complex",
    "duration",
    "stretch",
    "array",
    "qreg",
    "creg",
  ],
  constants: ["true", "false", "pi", "π", "tau", "τ", "euler", "ℇ"],
  operators: [
    "=",
    "==",
    "!=",
    "<",
    "<=",
    ">",
    ">=",
    "+",
    "++",
    "+=",
    "-",
    "-=",
    "*",
    "**",
    "*=",
    "/",
    "/=",
    "%",
    "^",
    "|",
    "||",
    "&",
    "&&",
    "!",
    "~",
    "<<",
    ">>",
    "->",
  ],
  symbols: /[=><!~?:&|+\-*/^%]+/,
  escapes: /\\[\s\S]/,
  tokenizer: {
    root: [
      // identifiers and keywords
      [
        /[a-zA-Z_$][\w$]*/,
        {
          cases: {
            "@typeKeywords": "type",
            "@keywords": "keyword",
            "@constants": "constant",
            "@default": "identifier",
          },
        },
      ],
      // whitespace and comments
      { include: "@whitespace" },
      // brackets and delimiters
      [/[{}()[\]]/, "@brackets"],
      [/@symbols/, { cases: { "@operators": "operator", "@default": "" } }],
      // numbers (including timing literals like 100ns, 10us, 5dt)
      [
        /\d*\.\d+([eE][-+]?\d+)?(ns|us|µs|ms|s|dt)?/,
        "number.float",
      ],
      [/\d+(ns|us|µs|ms|s|dt)?/, "number"],
      // delimiter: after number because of .\d floats
      [/[;,.]/, "delimiter"],
      // strings
      [/"/, { token: "string.quote", bracket: "@open", next: "@string" }],
    ],
    string: [
      [/[^\\"]+/, "string"],
      [/@escapes/, "string.escape"],
      [/"/, { token: "string.quote", bracket: "@close", next: "@pop" }],
    ],
    whitespace: [
      [/[ \t\r\n]+/, "white"],
      [/\/\*/, "comment", "@comment"],
      [/\/\/.*$/, "comment"],
    ],
    comment: [
      [/[^/*]+/, "comment"],
      [/\*\//, "comment", "@pop"],
      [/[/*]/, "comment"],
    ],
  },
};

let registered = false;

/**
 * Registers the "openqasm" language with Monaco for basic syntax highlighting.
 * Safe to call more than once; only the first call has an effect.
 */
export function registerOpenQasmLanguage() {
  if (registered) return;
  registered = true;
  monaco.languages.register({
    id: "openqasm",
    extensions: [".qasm", ".inc"],
    aliases: ["OpenQASM", "openqasm", "QASM"],
  });
  monaco.languages.setLanguageConfiguration("openqasm", configuration);
  monaco.languages.setMonarchTokensProvider("openqasm", language);
}
