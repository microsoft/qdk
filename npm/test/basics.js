// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//@ts-check

import assert from "node:assert";
import { test } from "node:test";
import { log } from "../dist/log.js";
import {
  getCompiler,
  getCompilerWorker,
  getLanguageService,
  getLanguageServiceWorker,
} from "../dist/main.js";
import { QscEventTarget } from "../dist/compiler/events.js";
import {
  getAllKatas,
  getExerciseDependencies,
  getKata,
} from "../dist/katas.js";
import samples from "../dist/samples.generated.js";

/** @type {import("../dist/log.js").TelemetryEvent[]} */
const telemetryEvents = [];
log.setLogLevel("warn");
log.setTelemetryCollector((event) => telemetryEvents.push(event));

/**
 *
 * @param {string} code
 * @param {string} expr
 * @param {boolean} useWorker
 * @returns {Promise<import("../dist/compiler/common.js").ShotResult>}
 */
export function runSingleShot(code, expr, useWorker) {
  return new Promise((resolve, reject) => {
    const resultsHandler = new QscEventTarget(true);
    const compiler = useWorker ? getCompilerWorker() : getCompiler();

    compiler
      .run(code, expr, 1, resultsHandler)
      .then(() => resolve(resultsHandler.getResults()[0]))
      .catch((err) => reject(err))
      /* @ts-expect-error: ICompiler does not include 'terminate' */
      .finally(() => (useWorker ? compiler.terminate() : null));
  });
}

test("basic eval", async () => {
  let code = `namespace Test {
        function Answer() : Int {
            return 42;
        }
    }`;
  let expr = `Test.Answer()`;

  const result = await runSingleShot(code, expr, false);
  assert(result.success);
  assert.equal(result.result, "42");
});

test("EntryPoint only", async () => {
  const code = `
namespace Test {
    @EntryPoint()
    operation MyEntry() : Result {
        use q1 = Qubit();
        return M(q1);
    }
}`;
  const result = await runSingleShot(code, "", true);
  assert(result.success === true);
  assert(result.result === "Zero");
});

test("dump and message output", async () => {
  let code = `namespace Test {
        function Answer() : Int {
            Microsoft.Quantum.Diagnostics.DumpMachine();
            Message("hello, qsharp");
            return 42;
        }
    }`;
  let expr = `Test.Answer()`;

  const result = await runSingleShot(code, expr, true);
  assert(result.success);
  assert(result.events.length == 2);
  assert(result.events[0].type == "DumpMachine");
  assert(result.events[0].state["|0⟩"].length == 2);
  assert(result.events[1].type == "Message");
  assert(result.events[1].message == "hello, qsharp");
});

async function validateExercise(exercise, code) {
  const evtTarget = new QscEventTarget(true);
  const compiler = getCompiler();
  const dependencies = await getExerciseDependencies(exercise);
  const success = await compiler.checkExerciseSolution(
    code,
    exercise.verificationCode,
    dependencies,
    evtTarget
  );

  const unsuccessful_events = evtTarget
    .getResults()
    .filter((evt) => !evt.success);
  let errorMsg = "";
  for (const event of unsuccessful_events) {
    const error = event.result;
    if (typeof error === "string") {
      errorMsg += "Result = " + error + "\n";
    } else {
      errorMsg += "Message = " + error.message + "\n";
    }
  }

  return {
    success: success,
    errorCount: unsuccessful_events.length,
    errorMsg: errorMsg,
  };
}

async function validateKata(kata) {
  const exercises = kata.sections.filter(
    (section) => section.type === "exercise"
  );
  for (const exercise of exercises) {
    const result = await validateExercise(exercise, exercise.placeholderCode);
    if (result.success || result.errorCount > 0) {
      console.log(
        `Exercise error (${exercise.id}): \n| ${result.success} \n| ${result.errorMsg}`
      );
    }
    assert(!result.success);
    assert(result.errorCount === 0);
  }
}

test("katas compile", async () => {
  const katas = await getAllKatas();
  for (const kata of katas) {
    await validateKata(kata);
  }
});

test("y_gate exercise", async () => {
  const code = `
    namespace Kata {
      operation ApplyY(q : Qubit) : Unit is Adj + Ctl {
        Y(q);
      }
    }`;
  const singleQubitGatesKata = await getKata("single_qubit_gates");
  const yGateExercise = singleQubitGatesKata.sections.find(
    (section) => section.type === "exercise" && section.id === "y_gate"
  );
  const result = await validateExercise(yGateExercise, code);
  assert(result.success);
});

test("worker 100 shots", async () => {
  let code = `namespace Test {
        function Answer() : Int {
            Microsoft.Quantum.Diagnostics.DumpMachine();
            Message("hello, qsharp");
            return 42;
        }
    }`;
  let expr = `Test.Answer()`;

  const resultsHandler = new QscEventTarget(true);
  const compiler = getCompilerWorker();
  await compiler.run(code, expr, 100, resultsHandler);
  compiler.terminate();

  const results = resultsHandler.getResults();

  assert.equal(results.length, 100);
  results.forEach((result) => {
    assert(result.success);
    assert.equal(result.result, "42");
    assert.equal(result.events.length, 2);
  });
});

test("Run samples", async () => {
  const compiler = getCompilerWorker();
  const resultsHandler = new QscEventTarget(true);

  for await (const sample of samples) {
    await compiler.run(sample.code, "", 1, resultsHandler);
  }

  compiler.terminate();
  assert.equal(resultsHandler.resultCount(), samples.length);
  resultsHandler.getResults().forEach((result) => {
    assert(result.success);
  });
});

test("state change", async () => {
  const compiler = getCompilerWorker();
  const resultsHandler = new QscEventTarget(false);
  const stateChanges = [];

  compiler.onstatechange = (state) => {
    stateChanges.push(state);
  };
  const code = `namespace Test {
    @EntryPoint()
    operation MyEntry() : Result {
        use q1 = Qubit();
        return M(q1);
    }
  }`;
  await compiler.run(code, "", 10, resultsHandler);
  compiler.terminate();
  // There SHOULDN'T be a race condition here between the 'run' promise completing and the
  // statechange events firing, as the run promise should 'resolve' in the next microtask,
  // whereas the idle event should fire synchronously when the queue is empty.
  // For more details, see https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Using_promises#task_queues_vs._microtasks
  assert(stateChanges.length === 2);
  assert(stateChanges[0] === "busy");
  assert(stateChanges[1] === "idle");
});

test("cancel worker", () => {
  return new Promise((resolve) => {
    const code = `namespace MyQuantumApp {
      open Microsoft.Quantum.Diagnostics;

      @EntryPoint()
      operation Main() : Result[] {
          repeat {} until false;
          return [];
      }
    }`;

    const cancelledArray = [];
    const compiler = getCompilerWorker();
    const resultsHandler = new QscEventTarget(false);

    // Queue some tasks that will never complete
    compiler.run(code, "", 10, resultsHandler).catch((err) => {
      cancelledArray.push(err);
    });
    compiler.getHir(code).catch((err) => {
      cancelledArray.push(err);
    });

    // Ensure those tasks are running/queued before terminating.
    setTimeout(async () => {
      // Terminate the compiler, which should reject the queued promises
      compiler.terminate();

      // Start a new compiler and ensure that works fine
      const compiler2 = getCompilerWorker();
      const result = await compiler2.getHir(code);
      compiler2.terminate();

      // getHir should have worked
      assert(typeof result === "string" && result.length > 0);

      // Old requests were cancelled
      assert(cancelledArray.length === 2);
      assert(cancelledArray[0] === "terminated");
      assert(cancelledArray[1] === "terminated");
      resolve(null);
    }, 4);
  });
});

test("check code", async () => {
  const compiler = getCompiler();

  const diags = await compiler.checkCode("namespace Foo []");
  assert.equal(diags.length, 1);
  assert.equal(diags[0].start_pos, 14);
  assert.equal(diags[0].end_pos, 15);
});

test("language service diagnostics", async () => {
  const languageService = getLanguageService();
  let gotDiagnostics = false;
  languageService.addEventListener("diagnostics", (event) => {
    gotDiagnostics = true;
    assert.equal(event.type, "diagnostics");
    assert.equal(event.detail.diagnostics.length, 1);
    assert.equal(
      event.detail.diagnostics[0].message,
      "type error: expected (Double, Qubit), found Qubit"
    );
  });
  await languageService.updateDocument(
    "test.qs",
    1,
    `namespace Sample {
    operation main() : Result[] {
        use q1 = Qubit();
        Ry(q1);
        let m1 = M(q1);
        return [m1];
    }
}`
  );
  assert(gotDiagnostics);
});

test("language service diagnostics - web worker", async () => {
  const languageService = getLanguageServiceWorker();
  let gotDiagnostics = false;
  languageService.addEventListener("diagnostics", (event) => {
    gotDiagnostics = true;
    assert.equal(event.type, "diagnostics");
    assert.equal(event.detail.diagnostics.length, 1);
    assert.equal(
      event.detail.diagnostics[0].message,
      "type error: expected (Double, Qubit), found Qubit"
    );
  });
  await languageService.updateDocument(
    "test.qs",
    1,
    `namespace Sample {
    operation main() : Result[] {
        use q1 = Qubit();
        Ry(q1);
        let m1 = M(q1);
        return [m1];
    }
}`
  );
  languageService.terminate();
  assert(gotDiagnostics);
});
async function testCompilerError(useWorker) {
  const compiler = useWorker ? getCompilerWorker() : getCompiler();
  if (useWorker) {
    // @ts-expect-error onstatechange only exists on the worker
    compiler.onstatechange = (state) => {
      lastState = state;
    };
  }

  const events = new QscEventTarget(true);
  let promiseResult = undefined;
  let lastState = undefined;
  await compiler
    .run("invalid code", "", 1, events)
    .then(() => {
      promiseResult = "success";
    })
    .catch(() => {
      promiseResult = "failure";
    });

  assert.equal(promiseResult, "failure");
  const results = events.getResults();
  assert.equal(results.length, 1);
  assert.equal(results[0].success, false);
  if (useWorker) {
    // Only the worker has state change events
    assert.equal(lastState, "idle");
    // @ts-expect-error terminate() only exists on the worker
    compiler.terminate();
  }
}

test("compiler error on run", () => testCompilerError(false));
test("compiler error on run - worker", () => testCompilerError(true));
