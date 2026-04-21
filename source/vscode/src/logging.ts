// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { LogLevel, log } from "qsharp-lang";
import * as vscode from "vscode";

export interface Logging {
  setListener(listener: LogFunction): void;
  setLevel(level: LogLevel): void;
}

type LogFunction = (level: LogLevel, ...args: any[]) => void;

export function initOutputWindowLogger() {
  const output = vscode.window.createOutputChannel("Q#", { log: true });

  // Override the global logger with functions that write to the output channel.
  // Use arrow functions to ensure the output channel's 'this' context is preserved.
  log.error = (msg: any, ...args: any[]) => output.error(msg, ...args);
  log.warn = (msg: any, ...args: any[]) => output.warn(msg, ...args);
  log.info = (msg: any, ...args: any[]) => output.info(msg, ...args);
  log.debug = (msg: any, ...args: any[]) => output.debug(msg, ...args);
  log.trace = (msg: any, ...args: any[]) => output.trace(msg, ...args);

  // The numerical log levels for VS Code and qsharp don't match.
  function mapLogLevel(logLevel: vscode.LogLevel) {
    switch (logLevel) {
      case vscode.LogLevel.Off:
        return "off";
      case vscode.LogLevel.Trace:
        return "trace";
      case vscode.LogLevel.Debug:
        return "debug";
      case vscode.LogLevel.Info:
        return "info";
      case vscode.LogLevel.Warning:
        return "warn";
      case vscode.LogLevel.Error:
        return "error";
    }
  }

  log.setLogLevel(mapLogLevel(output.logLevel));
  output.onDidChangeLogLevel((level) => {
    log.setLogLevel(mapLogLevel(level));
  });
}

export function initLogForwarder(): Logging {
  log.error = (...args) => forwardLog("error", ...args);
  log.warn = (...args) => forwardLog("warn", ...args);
  log.info = (...args) => forwardLog("info", ...args);
  log.debug = (...args) => forwardLog("debug", ...args);
  log.trace = (...args) => forwardLog("trace", ...args);

  // Collect all logs from source
  log.setLogLevel("trace");

  let listener: LogFunction | undefined = undefined;
  const buffered: [LogLevel, any[]][] = [];
  const levels: LogLevel[] = ["off", "error", "warn", "info", "debug", "trace"];
  let logLevel = 0;

  function forwardLog(level: LogLevel, ...args: any[]) {
    if (listener) {
      if (logLevel >= levels.indexOf(level)) {
        listener(level, args);
      }
    } else {
      // Buffer logs until a listener is hooked up
      buffered.push([level, args]);
    }
  }

  return {
    setListener(newListener: LogFunction) {
      listener = newListener;
      // Forward the buffered events to the new listener
      buffered.forEach(([level, args]) => forwardLog(level, args));
      buffered.length = 0;
    },
    setLevel(level: LogLevel) {
      logLevel = levels.indexOf(level);
    },
  };
}
