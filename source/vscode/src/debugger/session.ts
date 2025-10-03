// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/* eslint-disable @typescript-eslint/no-unused-vars */

import * as vscode from "vscode";

import {
  Breakpoint,
  ExitedEvent,
  Handles,
  InitializedEvent,
  Logger,
  LoggingDebugSession,
  OutputEvent,
  Scope,
  Source,
  StackFrame,
  StoppedEvent,
  TerminatedEvent,
  Thread,
  logger,
} from "@vscode/debugadapter";
import { DebugProtocol } from "@vscode/debugprotocol";
import {
  IDebugServiceWorker,
  IStructStepResult,
  QscEventTarget,
  StepResultId,
  log,
} from "qsharp-lang";
import { updateCircuitPanel } from "../circuit";
import { basename, isQdkDocument, toVsCodeRange } from "../common";
import {
  DebugEvent,
  EventType,
  UserFlowStatus,
  sendTelemetryEvent,
} from "../telemetry";
import { getRandomGuid } from "../utils";
import { createDebugConsoleEventTarget } from "./output";
import { ILaunchRequestArguments } from "./types";
import { escapeHtml } from "markdown-it/lib/common/utils.mjs";
import { isPanelOpen } from "../webviewPanel";
import { FullProgramConfig } from "../programConfig";

const ErrorProgramHasErrors =
  "The program contains one or more compile errors and cannot run. See debug console for more details.";
const ErrorProgramMissingEntry =
  "The Q# program does not contain an entry point and cannot run. See debug console for more details.";
const SimulationCompleted = "simulation completed.";
const ConfigurationDelayMS = 1000;

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

interface IBreakpointLocationData {
  /** The range as seen by the TextDocument for the file (0-based line/column) */
  range: vscode.Range;
  /** The range as seen by the DAP client (1-based line/column) */
  uiLocation: DebugProtocol.BreakpointLocation;
  /** Contains range as seen by the DAP client (1-based line/column) */
  breakpoint: DebugProtocol.Breakpoint;
}

export class QscDebugSession extends LoggingDebugSession {
  private static threadID = 1;

  private readonly knownPaths = new Map<string, string>();

  private breakpointLocations: Map<string, IBreakpointLocationData[]>;
  private breakpoints: Map<string, DebugProtocol.Breakpoint[]>;
  private variableHandles = new Handles<
    ["locals" | "quantum" | "circuit", number]
  >();
  private failureMessage: string;
  private eventTarget: QscEventTarget;
  private supportsVariableType = false;
  private revealedCircuit = false;

  public constructor(
    private debugService: IDebugServiceWorker,
    private config: vscode.DebugConfiguration,
    private program: FullProgramConfig,
  ) {
    super();

    this.failureMessage = "";
    this.eventTarget = createDebugConsoleEventTarget((message) => {
      this.writeToStdOut(message);
    });

    this.breakpointLocations = new Map<string, IBreakpointLocationData[]>();
    this.breakpoints = new Map<string, DebugProtocol.Breakpoint[]>();
    this.setDebuggerLinesStartAt1(false);
    this.setDebuggerColumnsStartAt1(false);

    const allKnownSources = getAllSources(program);
    for (const source of allKnownSources) {
      const uri = vscode.Uri.parse(source[0], true);

      // In Debug Protocol requests, the VS Code debug adapter client
      // will strip file URIs to just the filesystem path.
      // Keep track of the filesystem paths we know about so that
      // we can resolve them back to the original URI when handling requests.
      // See `asUri()` for more details.
      if (uri.scheme === "file") {
        this.knownPaths.set(uri.fsPath, uri.toString());
      }
    }
  }

  public async init(associationId: string): Promise<void> {
    const start = performance.now();
    sendTelemetryEvent(EventType.InitializeRuntimeStart, { associationId }, {});
    const failureMessage = await this.debugService.loadProgram(
      this.program,
      this.config.entry,
    );

    if (failureMessage == "") {
      for (const [path, _contents] of this.program.packageGraphSources.root
        .sources) {
        const locations = await this.debugService.getBreakpoints(path);
        log.trace(`init breakpointLocations: %O`, locations);
        const mapped = locations.map((location) => {
          const uiLocation: DebugProtocol.BreakpointLocation = {
            line: this.convertDebuggerLineToClient(location.range.start.line),
            column: this.convertDebuggerColumnToClient(
              location.range.start.character,
            ),
            endLine: this.convertDebuggerLineToClient(location.range.end.line),
            endColumn: this.convertDebuggerColumnToClient(
              location.range.end.character,
            ),
          };
          return {
            range: toVsCodeRange(location.range),
            uiLocation,
            breakpoint: this.createBreakpoint(location.id, uiLocation),
          } as IBreakpointLocationData;
        });
        this.breakpointLocations.set(path, mapped);
      }
    } else {
      log.warn(`compilation failed. ${failureMessage}`);
      this.failureMessage = failureMessage;
    }
    sendTelemetryEvent(
      EventType.InitializeRuntimeEnd,
      {
        associationId,
        flowStatus:
          this.failureMessage === ""
            ? UserFlowStatus.Succeeded
            : UserFlowStatus.Failed,
      },
      { timeToCompleteMs: performance.now() - start },
    );
  }

  /**
   * The 'initialize' request is the first request called by the frontend
   * to interrogate the features the debug adapter provides.
   */
  protected initializeRequest(
    response: DebugProtocol.InitializeResponse,
    args: DebugProtocol.InitializeRequestArguments,
  ): void {
    this.supportsVariableType = args.supportsVariableType ?? false;

    // build and return the capabilities of this debug adapter:
    response.body = response.body || {};

    // the adapter implements the configurationDone request.
    response.body.supportsConfigurationDoneRequest = true;

    // make VS Code show a 'step back' button
    response.body.supportsStepBack = false;

    // make VS Code support data breakpoints
    response.body.supportsDataBreakpoints = false;

    // make VS Code support completion in REPL
    response.body.supportsCompletionsRequest = false;

    // the adapter defines two exceptions filters, one with support for conditions.
    response.body.supportsExceptionFilterOptions = false;

    // make VS Code send exceptionInfo request
    response.body.supportsExceptionInfoRequest = false;

    // make VS Code able to read and write variable memory
    response.body.supportsReadMemoryRequest = false;
    response.body.supportsWriteMemoryRequest = false;

    response.body.supportSuspendDebuggee = false;
    response.body.supportTerminateDebuggee = true;
    response.body.supportsFunctionBreakpoints = true;
    response.body.supportsRestartRequest = false;

    // make VS Code send the breakpointLocations request
    response.body.supportsBreakpointLocationsRequest = true;

    /* Settings that we need to eventually support: */

    // make VS Code send cancel request
    response.body.supportsCancelRequest = false;

    // make VS Code use 'evaluate' when hovering over source
    response.body.supportsEvaluateForHovers = false;

    response.body.supportsDelayedStackTraceLoading = false;

    // make VS Code provide "Step in Target" functionality
    response.body.supportsStepInTargetsRequest = false;

    // make VS Code send setVariable request
    response.body.supportsSetVariable = false;

    // make VS Code send setExpression request
    response.body.supportsSetExpression = false;

    // make VS Code send disassemble request
    response.body.supportsDisassembleRequest = false;
    response.body.supportsSteppingGranularity = false;

    response.body.supportsInstructionBreakpoints = false;

    this.sendResponse(response);

    // since this debug adapter can accept configuration requests like 'setBreakpoint' at any time,
    // we request them early by sending an 'initializeRequest' to the frontend.
    // The frontend will end the configuration sequence by calling 'configurationDone' request.
    this.sendEvent(new InitializedEvent());
  }

  /**
   * Called at the end of the configuration sequence.
   * Indicates that all breakpoints etc. have been sent to the DA and that the 'launch' can start.
   */
  protected configurationDoneRequest(
    response: DebugProtocol.ConfigurationDoneResponse,
    args: DebugProtocol.ConfigurationDoneArguments,
  ): void {
    super.configurationDoneRequest(response, args);

    // notify the launchRequest that configuration has finished
    this.emit("sessionConfigurationDone");
  }

  protected async launchRequest(
    response: DebugProtocol.LaunchResponse,
    args: ILaunchRequestArguments,
  ): Promise<void> {
    const associationId = getRandomGuid();
    sendTelemetryEvent(EventType.Launch, { associationId }, {});
    if (this.failureMessage != "") {
      log.info(
        "compilation failed. sending error response and stopping execution.",
      );
      this.writeToDebugConsole(this.failureMessage);
      this.sendErrorResponse(response, {
        id: -1,
        format: this.failureMessage.includes("Qsc.EntryPoint.NotFound")
          ? ErrorProgramMissingEntry
          : ErrorProgramHasErrors,
        showUser: true,
      });
      return;
    }

    // configure DAP logging
    logger.setup(
      args.trace ? Logger.LogLevel.Verbose : Logger.LogLevel.Stop,
      false,
    );

    // wait until configuration has finished (configurationDoneRequest has been called)
    const configurationDone: Promise<void> = new Promise((resolve, reject) => {
      this.once("sessionConfigurationDone", resolve);
    });
    await Promise.race([configurationDone, delay(ConfigurationDelayMS)]);

    // This needs to be done before we start executing below
    // in order to ensure that the eventTarget is ready to receive
    // events from the debug service. Otherwise, we may miss events
    // that are sent before the active debug session is set.
    log.trace(`sending launchRequest response`);
    this.sendResponse(response);

    if (args.noDebug) {
      log.trace(`Running without debugging`);
      await this.runWithoutDebugging(args, associationId);
    } else {
      log.trace(`Running with debugging`);
      if (this.config.stopOnEntry) {
        sendTelemetryEvent(
          EventType.DebugSessionEvent,
          { associationId, event: DebugEvent.StepIn },
          {},
        );
        await this.stepIn();
      } else {
        sendTelemetryEvent(
          EventType.DebugSessionEvent,
          { associationId, event: DebugEvent.Continue },
          {},
        );
        await this.continue();
      }
    }
  }

  private async eval_step(step: () => Promise<IStructStepResult>) {
    let result: IStructStepResult | undefined;
    let error;
    try {
      result = await step();
    } catch (e) {
      error = e;
    }

    await this.updateCircuit(error);

    if (!result) {
      // Can be a runtime failure in the program
      await this.endSession(`ending session due to error: ${error}`, 1);
      return;
    } else if (result.id == StepResultId.BreakpointHit) {
      const evt = new StoppedEvent(
        "breakpoint",
        QscDebugSession.threadID,
      ) as DebugProtocol.StoppedEvent;
      evt.body.hitBreakpointIds = [result.value];
      log.trace(`raising breakpoint event`);
      this.sendEvent(evt);
    } else if (result.id == StepResultId.Return) {
      await this.endSession(`ending session`, 0);
    } else if (result.id == StepResultId.Fail) {
      log.trace(
        `step result: ${result.id} ${result.value} (error: ${result.error})`,
      );
      this.sendEvent(
        new StoppedEvent("exception", QscDebugSession.threadID, result.error),
      );
    } else {
      log.trace(`step result: ${result.id} ${result.value}`);
      this.sendEvent(new StoppedEvent("step", QscDebugSession.threadID));
    }
  }

  private async continue(): Promise<void> {
    const bps = this.getBreakpointIds();
    await this.eval_step(
      async () => await this.debugService.evalContinue(bps, this.eventTarget),
    );
  }

  private async next(): Promise<void> {
    const bps = this.getBreakpointIds();
    await this.eval_step(
      async () => await this.debugService.evalNext(bps, this.eventTarget),
    );
  }

  private async stepIn(): Promise<void> {
    const bps = this.getBreakpointIds();
    await this.eval_step(
      async () => await this.debugService.evalStepIn(bps, this.eventTarget),
    );
  }

  private async stepOut(): Promise<void> {
    const bps = this.getBreakpointIds();
    await this.eval_step(
      async () => await this.debugService.evalStepOut(bps, this.eventTarget),
    );
  }

  private async endSession(message: string, exitCode: number): Promise<void> {
    log.trace(message);
    this.writeToDebugConsole("");
    this.writeToDebugConsole(SimulationCompleted);
    this.sendEvent(new TerminatedEvent());
    this.sendEvent(new ExitedEvent(exitCode));
  }

  private async runWithoutDebugging(
    args: ILaunchRequestArguments,
    associationId: string,
  ): Promise<void> {
    const bps: number[] = [];
    // This will be replaced when the interpreter
    // supports shots.
    for (let i = 0; i < args.shots; i++) {
      try {
        let result = await this.debugService.evalContinue(
          bps,
          this.eventTarget,
        );

        // Ensure the circuit is updated before checking for fail, since the
        // right-hand side of the fail expression may itself be a block that includes
        // gate calls.
        await this.updateCircuit();

        if (result.id == StepResultId.Fail) {
          // This was a runtime failure in the program, so we need to continue to
          // trigger the actual failure event.
          result = await this.debugService.evalContinue(bps, this.eventTarget);
        }

        if (result.id != StepResultId.Return) {
          await this.endSession(`execution didn't run to completion`, -1);
          return;
        }
      } catch (error) {
        await this.updateCircuit(error);
        await this.endSession(`ending session due to error: ${error}`, 1);
        return;
      }

      this.writeToDebugConsole(`Finished shot ${i + 1} of ${args.shots}`);
      // Reset the interpreter for the next shot.
      // The interactive interpreter doesn't do this automatically,
      // and doesn't know how to deal with shots like the stateless version.
      await this.init(associationId);
      if (this.failureMessage != "") {
        log.info(
          "compilation failed. sending error response and stopping execution.",
        );
        this.writeToDebugConsole(this.failureMessage);
        await this.endSession(`ending session`, -1);
        return;
      }
    }
    await this.endSession(`ending session`, 0);
  }

  private getBreakpointIds(): number[] {
    const bps: number[] = [];
    for (const file_bps of this.breakpoints.values()) {
      for (const bp of file_bps) {
        if (bp?.id != null) {
          bps.push(bp.id);
        }
      }
    }

    return bps;
  }

  protected async continueRequest(
    response: DebugProtocol.ContinueResponse,
    args: DebugProtocol.ContinueArguments,
  ): Promise<void> {
    log.trace(`continueRequest: %O`, args);

    log.trace(`sending continue response`);
    this.sendResponse(response);

    await this.continue();
  }

  protected async nextRequest(
    response: DebugProtocol.NextResponse,
    args: DebugProtocol.NextArguments,
    request?: DebugProtocol.Request,
  ): Promise<void> {
    log.trace(`nextRequest: %O`, args);

    this.sendResponse(response);
    await this.next();
  }

  protected async stepInRequest(
    response: DebugProtocol.StepInResponse,
    args: DebugProtocol.StepInArguments,
    request?: DebugProtocol.Request,
  ): Promise<void> {
    log.trace(`stepInRequest: %O`, args);
    this.sendResponse(response);

    await this.stepIn();
  }

  protected async stepOutRequest(
    response: DebugProtocol.StepOutResponse,
    args: DebugProtocol.StepOutArguments,
    request?: DebugProtocol.Request,
  ): Promise<void> {
    log.trace(`stepOutRequest: %O`, args);
    this.sendResponse(response);

    await this.stepOut();
  }

  protected async breakpointLocationsRequest(
    response: DebugProtocol.BreakpointLocationsResponse,
    args: DebugProtocol.BreakpointLocationsArguments,
    request?: DebugProtocol.Request,
  ): Promise<void> {
    log.trace(`breakpointLocationsRequest: %O`, args);

    response.body = {
      breakpoints: [],
    };

    const doc = await this.tryLoadSource(args.source);
    log.trace(
      `breakpointLocationsRequest: path=${args.source.path} resolved to ${doc?.uri}`,
    );

    // If we couldn't find the document, or if
    // the range is longer than the document, just return
    const targetLineNumber = this.convertClientLineToDebugger(args.line);
    if (!doc || targetLineNumber >= doc.lineCount) {
      log.trace(`setBreakPointsResponse: %O`, response);
      this.sendResponse(response);
      return;
    }

    // Map request start/end line/column to file offset for debugger
    // everything from `file` is 0 based, everything from `args` is 1 based
    // so we have to convert anything from `args` to 0 based

    const line = doc.lineAt(targetLineNumber);
    const lineRange = line.range;
    // If the column isn't specified, it is a line breakpoint so that we
    // use the whole line's range for breakpoint finding.
    const isLineBreakpoint = !args.column;
    const startLine = lineRange.start.line;
    // If the column isn't specified, use the start of the line. This also means
    // that we are looking at the whole line for a breakpoint
    const startCol = args.column
      ? this.convertClientColumnToDebugger(args.column)
      : lineRange.start.character;
    // If the end line isn't specified, use the end of the line range
    const endLine = args.endLine
      ? this.convertClientLineToDebugger(args.endLine)
      : lineRange.end.line;
    // If the end column isn't specified, use the end of the line.
    const endCol = args.endColumn
      ? this.convertClientColumnToDebugger(args.endColumn)
      : lineRange.end.character;

    // We've translated the request's range into a full implied range,
    // which can be used to isolate statements.
    const requestRange = new vscode.Range(startLine, startCol, endLine, endCol);

    log.trace(
      `breakpointLocationsRequest: ${startLine}:${startCol} - ${endLine}:${endCol}`,
    );

    // Now that we have the mapped breakpoint span, get the potential
    // breakpoints from the debugger

    // If is is a line breakpoint, we can just use the line number for matching
    // Otherwise, when looking for range breakpoints, we are given a single
    // column offset, so we need to check if the startOffset is within range.
    const bps =
      this.breakpointLocations
        .get(doc.uri.toString())
        ?.filter((bp) =>
          isLineBreakpoint
            ? bp.range.start.line == requestRange.start.line
            : requestRange.contains(bp.range.start),
        ) ?? [];

    log.trace(`breakpointLocationsRequest: candidates %O`, bps);

    // must map the debugger breakpoints back to the client breakpoint locations
    const bls = bps.map((bps) => {
      return bps.uiLocation;
    });
    log.trace(`breakpointLocationsRequest: mapped %O`, bls);
    response.body = {
      breakpoints: bls,
    };

    log.trace(`breakpointLocationsResponse: %O`, response);
    this.sendResponse(response);
  }

  protected async setBreakPointsRequest(
    response: DebugProtocol.SetBreakpointsResponse,
    args: DebugProtocol.SetBreakpointsArguments,
    request?: DebugProtocol.Request,
  ): Promise<void> {
    log.trace(`setBreakPointsRequest: %O`, args);

    const doc = await this.tryLoadSource(args.source);
    log.trace(
      `setBreakPointsRequest: path=${args.source.path} resolved to ${doc?.uri}`,
    );

    // If we couldn't find the document, just return
    if (!doc) {
      log.trace(`setBreakPointsResponse: %O`, response);
      this.sendResponse(response);
      return;
    }

    log.trace(`setBreakPointsRequest: looking`);
    this.breakpoints.set(doc.uri.toString(), []);
    log.trace(
      `setBreakPointsRequest: files in cache %O`,
      this.breakpointLocations.keys(),
    );
    const locations = this.breakpointLocations.get(doc.uri.toString()) ?? [];
    log.trace(`setBreakPointsRequest: got locations %O`, locations);
    const desiredBpOffsets: {
      range: vscode.Range;
      isLineBreakpoint: boolean;
      uiLine: number;
    }[] = (args.breakpoints ?? [])
      .filter(
        (sourceBreakpoint) =>
          this.convertClientLineToDebugger(sourceBreakpoint.line) <
          doc.lineCount,
      )
      .map((sourceBreakpoint) => {
        const isLineBreakpoint = !sourceBreakpoint.column;
        const line = this.convertClientLineToDebugger(sourceBreakpoint.line);
        const lineRange = doc.lineAt(line).range;
        const startCol = sourceBreakpoint.column
          ? this.convertClientColumnToDebugger(sourceBreakpoint.column)
          : lineRange.start.character;
        const startPos = new vscode.Position(line, startCol);

        return {
          range: new vscode.Range(startPos, lineRange.end),
          isLineBreakpoint,
          uiLine: sourceBreakpoint.line,
        };
      });

    // Now that we have the mapped breakpoint span, get the actual breakpoints
    // with corresponding ids from the debugger
    const bps = [];

    for (const bpOffset of desiredBpOffsets) {
      const lo = bpOffset.range.start;
      const isLineBreakpoint = bpOffset.isLineBreakpoint;
      const uiLine = bpOffset.uiLine;
      // we can quickly filter out any breakpoints that are outside of the
      // desired line
      const matchingLocations = locations.filter((location) => {
        return location.uiLocation.line == uiLine;
      });
      // Now if the breakpoint is a line breakpoint, we can just use the first
      // matching location. Otherwise, we need to check if the desired column
      // is within the range of the location.
      for (const location of matchingLocations) {
        if (isLineBreakpoint) {
          bps.push(location.breakpoint);
        } else {
          // column bp just has end of selection or cursor location in lo
          if (location.range.contains(lo)) {
            bps.push(location.breakpoint);
          }
        }
      }
    }

    // Update our breakpoint list for the given file
    this.breakpoints.set(doc.uri.toString(), bps);

    response.body = {
      breakpoints: bps,
    };

    log.trace(`setBreakPointsResponse: %O`, response);
    this.sendResponse(response);
  }

  protected threadsRequest(response: DebugProtocol.ThreadsResponse): void {
    log.trace(`threadRequest`);
    response.body = {
      threads: [new Thread(QscDebugSession.threadID, "thread 1")],
    };
    log.trace(`threadResponse: %O`, response);
    this.sendResponse(response);
  }

  protected async stackTraceRequest(
    response: DebugProtocol.StackTraceResponse,
    args: DebugProtocol.StackTraceArguments,
  ): Promise<void> {
    log.trace(`stackTraceRequest: %O`, args);
    const debuggerStackFrames = await this.debugService.getStackFrames();
    log.trace(`frames: %O`, debuggerStackFrames);
    const filterUndefined = <V>(value: V | undefined): value is V =>
      value != null;
    const mappedStackFrames = await Promise.all(
      debuggerStackFrames
        .map(async (f, id) => {
          log.trace(`frames: location %O`, f.location);

          const uri = f.location.source;
          const sf: DebugProtocol.StackFrame = new StackFrame(
            id,
            f.name,
            new Source(
              basename(vscode.Uri.parse(uri).path) ?? uri,
              uri,
              undefined,
              undefined,
              "qsharp-adapter-data",
            ),
            this.convertDebuggerLineToClient(f.location.span.start.line),
            this.convertDebuggerColumnToClient(f.location.span.start.character),
          );
          sf.endLine = this.convertDebuggerLineToClient(
            f.location.span.end.line,
          );
          sf.endColumn = this.convertDebuggerColumnToClient(
            f.location.span.end.character,
          );
          return sf;
        })
        .filter(filterUndefined),
    );
    const stackFrames = mappedStackFrames.reverse();
    stackFrames.push(
      new StackFrame(0, "entry", undefined) as DebugProtocol.StackFrame,
    );
    response.body = {
      stackFrames: stackFrames,
      totalFrames: stackFrames.length,
    };

    log.trace(`stackTraceResponse: %O`, response);
    this.sendResponse(response);
  }

  protected disconnectRequest(
    response: DebugProtocol.DisconnectResponse,
    args: DebugProtocol.DisconnectArguments,
    request?: DebugProtocol.Request,
  ): void {
    log.trace(`disconnectRequest: %O`, args);
    this.debugService.terminate();
    this.sendResponse(response);
    log.trace(`disconnectResponse: %O`, response);
  }

  protected scopesRequest(
    response: DebugProtocol.ScopesResponse,
    args: DebugProtocol.ScopesArguments,
  ): void {
    log.trace(`scopesRequest: %O`, args);
    response.body = {
      scopes: [
        new Scope(
          "Locals",
          this.variableHandles.create(["locals", args.frameId]),
          false,
        ),
        new Scope(
          "Quantum State",
          this.variableHandles.create(["quantum", -1]),
          true, // expensive - keeps scope collapsed in the UI by default
        ),
        new Scope(
          "Quantum Circuit",
          this.variableHandles.create(["circuit", -1]),
          true, // expensive - keeps scope collapsed in the UI by default
        ),
      ],
    };
    log.trace(`scopesResponse: %O`, response);
    this.sendResponse(response);
  }

  protected async variablesRequest(
    response: DebugProtocol.VariablesResponse,
    args: DebugProtocol.VariablesArguments,
    request?: DebugProtocol.Request,
  ): Promise<void> {
    log.trace(`variablesRequest: ${JSON.stringify(args, null, 2)}`);

    response.body = {
      variables: [],
    };

    const [handle, frameID] = this.variableHandles.get(args.variablesReference);

    switch (handle) {
      case "locals":
        {
          const locals = await this.debugService.getLocalVariables(frameID);
          const variables = locals.map((local) => {
            const variable: DebugProtocol.Variable = {
              name: local.name,
              value: local.value,
              variablesReference: 0,
            };
            if (this.supportsVariableType) {
              variable.type = local.var_type;
            }
            return variable;
          });
          response.body = {
            variables: variables,
          };
        }
        break;
      case "quantum":
        {
          const associationId = getRandomGuid();
          const start = performance.now();
          sendTelemetryEvent(
            EventType.RenderQuantumStateStart,
            { associationId },
            {},
          );
          const state = await this.debugService.captureQuantumState();

          // When there is no quantum state, return a single variable with a message
          // for the user.
          if (state.length == 0) {
            response.body = {
              variables: [
                {
                  name: "None",
                  value: "No qubits allocated",
                  variablesReference: 0,
                },
              ],
            };
            break;
          }

          const variables: DebugProtocol.Variable[] = state.map((entry) => {
            const variable: DebugProtocol.Variable = {
              name: entry.name,
              value: entry.value,
              variablesReference: 0,
              type: "Complex",
            };
            return variable;
          });
          sendTelemetryEvent(
            EventType.RenderQuantumStateEnd,
            { associationId },
            { timeToCompleteMs: performance.now() - start },
          );
          response.body = {
            variables: variables,
          };
        }
        break;
      case "circuit":
        {
          // This will get invoked when the "Quantum Circuit" scope is expanded
          // in the Variables view, but instead of showing any values in the variables
          // view, we can pop open the circuit diagram panel.
          if (!this.config.showCircuit) {
            // Keep updating the circuit for the rest of this session, even if
            // the Variables scope gets collapsed by the user. If we don't do this,
            // the diagram won't get updated with each step even though the circuit
            // panel is still being shown, which is misleading.
            this.config.showCircuit = true;
            await this.updateCircuit();
          }
          response.body = {
            variables: [
              {
                name: "Circuit",
                value: "See QDK Circuit panel",
                variablesReference: 0,
              },
            ],
          };
        }
        break;
    }

    log.trace(`variablesResponse: %O`, response);
    this.sendResponse(response);
  }

  private createBreakpoint(
    id: number,
    location: DebugProtocol.BreakpointLocation,
  ): DebugProtocol.Breakpoint {
    const verified = true;
    const bp = new Breakpoint(verified) as DebugProtocol.Breakpoint;
    bp.id = id;
    bp.line = location.line;
    bp.column = location.column;
    bp.endLine = location.endLine;
    bp.endColumn = location.endColumn;
    return bp;
  }

  private writeToStdOut(message: string): void {
    const evt: DebugProtocol.OutputEvent = new OutputEvent(
      `${message}\n`,
      "stdout",
    );
    this.sendEvent(evt);
  }

  private writeToDebugConsole(message: string): void {
    const evt: DebugProtocol.OutputEvent = new OutputEvent(
      `${message}\n`,
      "console",
    );
    this.sendEvent(evt);
  }

  /**
   * Attempts to find the Source in the current session and returns the
   * TextDocument if it exists.
   *
   * This method *may* return a valid result even when the requested
   * path does not belong in the current program (e.g. another Q# file
   * in the workspace).
   */
  async tryLoadSource(source: DebugProtocol.Source) {
    if (!source.path) {
      return;
    }

    const uri = this.asUri(source.path);
    if (!uri) {
      return;
    }

    try {
      const doc = await vscode.workspace.openTextDocument(uri);
      if (isQdkDocument(doc)) {
        return doc;
      }
      return undefined;
    } catch (e) {
      log.trace(`Failed to open ${uri}: ${e}`);
    }
  }

  /**
   * Attempts to resolve a DebugProtocol.Source.path to a URI.
   *
   * In Debug Protocol requests, the VS Code debug adapter client
   * will strip file URIs to just the filesystem path part.
   * But for non-file URIs, the full URI is sent.
   *
   * See: https://github.com/microsoft/vscode/blob/3246d63177e1e5ae211029e7ab0021c33342a3c7/src/vs/workbench/contrib/debug/common/debugSource.ts#L90
   *
   * Here, we need the original URI, but we don't know if we're
   * dealing with a filesystem path or URI. We cannot determine
   * which one it is based on the input alone (the syntax is ambiguous).
   * But we do have a set of *known* filesystem paths that we
   * constructed at initialization, and we can use that to resolve
   * any known filesystem paths back to the original URI.
   *
   * Filesystem paths we don't know about *won't* be resolved,
   * and that's ok in this use case.
   *
   * If the path was originally constructed from a URI, it won't
   * be in our known paths map, so we'll treat the string as a URI.
   */
  asUri(pathOrUri: string): vscode.Uri | undefined {
    pathOrUri = this.knownPaths.get(pathOrUri) || pathOrUri;

    try {
      return vscode.Uri.parse(pathOrUri);
    } catch (e) {
      log.trace(`Could not resolve path ${pathOrUri}`);
    }
  }

  /* Updates the circuit panel if `showCircuit` is true or if panel is already open */
  private async updateCircuit(error?: any) {
    if (
      this.config.showCircuit ||
      isPanelOpen("circuit", this.program.projectName)
    ) {
      // Error returned from the debugger has a message and a stack (which also includes the message).
      // We would ideally retrieve the original runtime error, and format it to be consistent
      // with the other runtime errors that can be shown in the circuit panel, but that will require
      // a bit of refactoring.
      const stack =
        error && typeof error === "object" && typeof error.stack === "string"
          ? escapeHtml(error.stack)
          : undefined;

      const circuit = await this.debugService.getCircuit();

      updateCircuitPanel(
        this.program.profile,
        this.program.projectName,
        !this.revealedCircuit,
        {
          circuit,
          errorHtml: stack ? `<pre>${stack}</pre>` : undefined,
          simulated: true,
        },
      );

      // Only reveal the panel once per session, to keep it from
      // moving around while stepping
      this.revealedCircuit = true;
    }
  }
}

function getAllSources(program: FullProgramConfig) {
  return program.packageGraphSources.root.sources.concat(
    Object.values(program.packageGraphSources.packages).flatMap(
      (p) => p.sources,
    ),
  );
}
