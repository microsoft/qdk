// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Logging infrastructure for JavaScript environments (e.g. browser and node.js)
//
// Ideally this should be the only module to have global side effects and run code
// on module load (i.e. other modules should consist almost entirely of declarations
// and exports at the top level), which means it is configurable and usable from
// the host environment after import resolution and before other logic runs.

declare global {
  // Align with VS Code names (but not level numbers)
  // 0 = off
  // 1 = error
  // 2 = warn
  // 3 = info
  // 4 = debug (called 'verbose' in VS Code)
  // 5 = trace
  // Note this also aligns with the Rust log crate macros/levels
  // See https://docs.rs/log/latest/log/
  var qscLog: typeof log;
  var qscGitHash: string;
}

export type LogLevel = "off" | "error" | "warn" | "info" | "debug" | "trace";
export type TelemetryEvent = { id: string; data?: any };
export type TelemetryCollector = (event: TelemetryEvent) => void;

let telemetryCollector: TelemetryCollector | null = null;
const levels = ["off", "error", "warn", "info", "debug", "trace"];
let logLevel = 0;

export const log = {
  setLogLevel(level: LogLevel | number) {
    if (typeof level === "string") {
      // Convert to number
      const lowerLevel = level.toLowerCase();
      let newLevel = 0;
      levels.forEach((name, idx) => {
        if (name === lowerLevel) newLevel = idx;
      });
      logLevel = newLevel;
    } else {
      logLevel = level;
    }
    this.onLevelChanged?.(logLevel);
  },
  onLevelChanged: null as ((level: number) => void) | null,
  getLogLevel(): number {
    return logLevel;
  },
  error(...args: any) {
    if (logLevel >= 1) console.error(...args);
  },
  warn(...args: any) {
    if (logLevel >= 2) console.warn(...args);
  },
  info(...args: any) {
    if (logLevel >= 3) console.info(...args);
  },
  debug(...args: any) {
    if (logLevel >= 4) console.debug(...args);
  },
  trace(...args: any) {
    // console.trace in JavaScript just writes a stack trace at info level, so use 'debug'
    if (logLevel >= 5) console.debug(...args);
  },
  never(val: never) {
    // Utility function to ensure exhaustive type checking. See https://stackoverflow.com/a/39419171
    log.error("Exhaustive type checking didn't account for: %o", val);
  },
  /**
   * @param level - A number indicating severity: 1 = Error, 2 = Warn, 3 = Info, 4 = Debug, 5 = Trace
   * @param target - The area or component sending the message, e.g. "parser" (useful for filtering)
   * @param args - The format string and args to log, e.g. ["Index of %s is %i", str, index]
   */
  logWithLevel(level: number, target: string, ...args: any) {
    const [firstArg, ...trailingArgs] = args;
    const outArgs = [`[${target || ""}] ${firstArg}`, ...trailingArgs];
    switch (level) {
      case 1:
        log.error(...outArgs);
        break;
      case 2:
        log.warn(...outArgs);
        break;
      case 3:
        log.info(...outArgs);
        break;
      case 4:
        log.debug(...outArgs);
        break;
      case 5:
        log.trace(...outArgs);
        break;
      default:
        log.error("Invalid logLevel: ", level);
    }
  },
  setTelemetryCollector(handler: TelemetryCollector) {
    telemetryCollector = handler;
  },
  logTelemetry(event: { id: string; data?: any }) {
    telemetryCollector?.(event);
  },
  isTelemetryEnabled() {
    return !!telemetryCollector;
  },
};

// Enable globally for easy interaction and debugging in live environments
globalThis.qscLog = log;
