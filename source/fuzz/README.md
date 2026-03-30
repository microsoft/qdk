# Fuzzing

Based on [Fuzzing with cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html).

For running locally you need the following steps.

(**On Windows use [WSL](https://learn.microsoft.com/windows/wsl/).** Tested in WSL Ubuntu 22.04)

## Prerequisites

```bash
rustup install nightly
rustup default nightly

cargo install cargo-fuzz
```

## Running

**NOTE:** All the commands below are executed in WSL in the directory that contains this "fuzz" directory
in a clean local copy of the repo (the whole repo is not built).

```bash
cargo fuzz list                         # Optional. See the available fuzzing targets.
# compile                               # This fuzzing target fuzzes the `compile()` function.
cargo fuzz run compile --features do_fuzz -- -seed_inputs=@fuzz/seed_inputs/compile/list.txt
                                        # Build and run the fuzzing target "compile".
# The build takes a few minutes. You may get an impression that the build takes place twice,
# that is expected.
# The run/fuzzing can last indefinitely without bumping into a panic. Stop with <Ctrl+c>.

cargo fuzz run compile --features do_fuzz -- -help=1       # Optional. See the available run settings.

# Optional. Run fuzzing for 10 runs at most, 5 seconds at most, generate ASCII-only fuzzing sequences.
cargo fuzz run compile --features do_fuzz -- -seed_inputs=@fuzz/seed_inputs/compile/list.txt -runs=10 -max_total_time=5 -only_ascii=1
```

## Purifying the Bugs Found with Fuzzing

<details><summary>The commands below were executed in a branch based on the following commit in "main" (click this line).</summary>

```log
commit e51a8b6f145be23fc2358b2cf0bab6707a7a46a0 (origin/main, origin/HEAD, main)
Author: Bill Ticehurst <billti@microsoft.com>
Date:   Wed Apr 19 10:42:03 2023 -0700

    Fix mapping of spans for non-ASCII code (#182)

    This builds on the branch for the PR at
    https://github.com/microsoft/qdk/pull/180 (which fixes the code
    sharing issue with non-ASCII chars), not not strictly dependent.

    The excessive comments on the `mapUtf8UnitsToUtf16Units` function should
    outline why this is needed and what it fixes.
```

</details>

The fuzzing run `cargo fuzz run compile`, if hits a panic, reports the panic message

```log
thread '<unnamed>' panicked at 'local variable should have inferred type', \
    .../qsharp/source/compiler/qsc_frontend/src/typeck/rules.rs:326:30
```

Among the last few lines the log lists the following commands of interest:

```log
Reproduce with:
    cargo fuzz run compile fuzz/artifacts/compile/crash-22fc59256083904ead44f3ce8f5f04a251d7cc23
Minimize test case with:
    cargo fuzz tmin compile fuzz/artifacts/compile/crash-22fc59256083904ead44f3ce8f5f04a251d7cc23
```

The first thing you may typically do is to look at the input that caused the panic:  
`cat fuzz/artifacts/compile/crash-22fc59256083904ead44f3ce8f5f04a251d7cc23`.  
The input may be longer than sufficient to cause the panic. So the next thing you may want to do
is to shorten the input (see the "Minimize test case with" above), but it is recommended to
add `-r 10000` after `tmin` (which results in a longer run but much shorter input, in any case the run takes within one minute):  
`cargo fuzz tmin -r 10000 compile fuzz/artifacts/compile/crash-22fc59256083904ead44f3ce8f5f04a251d7cc23`.  
This command makes a number of runs with shorter input to figure out a shorter sequence that causes the panic.  
The log fragments of interest are in the end:

```log
Minimized artifact:
    fuzz/artifacts/compile/minimized-from-b665e6267c297608e85c5948481cd353107a07fa
```

```log
Reproduce with:
    cargo fuzz run compile fuzz/artifacts/compile/minimized-from-b665e6267c297608e85c5948481cd353107a07fa
```

**NOTE:** This command of automated input shortening can end up in a _different panic_.  
That panic can be a new bug found or a previously known bug.

Make sure that you are still on track, reproduce the panic of interest with the shortened input (see "Reproduce with" command above):

```bash
cargo fuzz run compile --features do_fuzz fuzz/artifacts/compile/minimized-from-b665e6267c297608e85c5948481cd353107a07fa
```

Right below the panic message the log gives you a stack trace hint:  
`note: run with 'RUST_BACKTRACE=1' environment variable to display a backtrace`.

You can enable the stack trace display, when reproducing the panic, like this:

```bash
RUST_BACKTRACE=1 cargo fuzz run compile --features do_fuzz fuzz/artifacts/compile/minimized-from-b665e6267c297608e85c5948481cd353107a07fa
```

(you repeat the repro command but you set the environment variable `RUST_BACKTRACE` for that run only).

**NOTE:** See the stack trace shown not in the end of the repro log, but immediately after the panic message.

<details><summary>Example (click this line).</summary>

```log
thread 'unnamed' panicked at 'local variable should have inferred type', /mnt/c/ed/dev/QSharpCompiler/qsharp-runtime/qsharp/source/compiler/qsc_frontend/src/typeck/rules.rs:326:30
stack backtrace:
   0: rust_begin_unwind
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/std/src/panicking.rs:577:5
   1: core::panicking::panic_fmt
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/core/src/panicking.rs:67:14
   2: core::panicking::panic_display
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/core/src/panicking.rs:150:5
   3: core::panicking::panic_str
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/core/src/panicking.rs:134:5
   4: core::option::expect_failed
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/core/src/option.rs:2025:5
   5: core::option::Option{T}::expect
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/core/src/option.rs:913:21
   6: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:326:30
   7: qsc_frontend::typeck::rules::Context::infer_binop
             at ./src/typeck/rules.rs:445:32
   8: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:217:56
   9: qsc_frontend::typeck::rules::Context::infer_binop
             at ./src/typeck/rules.rs:444:32
  10: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:217:56
  11: qsc_frontend::typeck::rules::Context::infer_update
             at ./src/typeck/rules.rs:509:38
  12: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:368:27
  13: qsc_frontend::typeck::rules::Context::infer_update
             at ./src/typeck/rules.rs:509:38
  14: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:368:27
  15: qsc_frontend::typeck::rules::Context::infer_update
             at ./src/typeck/rules.rs:509:38
  16: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:368:27
  17: qsc_frontend::typeck::rules::Context::infer_update
             at ./src/typeck/rules.rs:509:38
  18: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:368:27
  19: qsc_frontend::typeck::rules::Context::infer_update
             at ./src/typeck/rules.rs:509:38
  20: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:368:27
  21: qsc_frontend::typeck::rules::Context::infer_update
             at ./src/typeck/rules.rs:509:38
  22: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:368:27
  23: qsc_frontend::typeck::rules::Context::infer_update
             at ./src/typeck/rules.rs:509:38
  24: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:368:27
  25: qsc_frontend::typeck::rules::Context::infer_update
             at ./src/typeck/rules.rs:509:38
  26: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:368:27
  27: qsc_frontend::typeck::rules::Context::infer_expr
             at ./src/typeck/rules.rs:351:26
  28: qsc_frontend::typeck::rules::Context::infer_stmt
             at ./src/typeck/rules.rs:172:27
  29: qsc_frontend::typeck::rules::Context::infer_block
             at ./src/typeck/rules.rs:143:35
  30: qsc_frontend::typeck::rules::Context::infer_spec
             at ./src/typeck/rules.rs:106:21
  31: qsc_frontend::typeck::rules::spec
             at ./src/typeck/rules.rs:610:5
  32: qsc_frontend::typeck::check::Checker::check_spec
             at ./src/typeck/check.rs:105:22
  33: {qsc_frontend::typeck::check::Checker as qsc_ast::visit::Visitor}::visit_callable_decl
             at ./src/typeck/check.rs:131:48
  34: qsc_ast::visit::walk_item
             at /mnt/c/ed/dev/QSharpCompiler/qsharp-runtime/qsharp/source/compiler/qsc_ast/src/visit.rs:94:37
  35: qsc_ast::visit::Visitor::visit_item
             at /mnt/c/ed/dev/QSharpCompiler/qsharp-runtime/qsharp/source/compiler/qsc_ast/src/visit.rs:20:9
  36: qsc_ast::visit::walk_namespace::{{closure}}
             at /mnt/c/ed/dev/QSharpCompiler/qsharp-runtime/qsharp/source/compiler/qsc_ast/src/visit.rs:86:41
  37: {core::slice::iter::Iter{T} as core::iter::traits::iterator::Iterator}::for_each
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/core/src/slice/iter/macros.rs:201:21
  38: qsc_ast::visit::walk_namespace
             at /mnt/c/ed/dev/QSharpCompiler/qsharp-runtime/qsharp/source/compiler/qsc_ast/src/visit.rs:86:5
  39: qsc_ast::visit::Visitor::visit_namespace
             at /mnt/c/ed/dev/QSharpCompiler/qsharp-runtime/qsharp/source/compiler/qsc_ast/src/visit.rs:16:9
  40: {qsc_frontend::typeck::check::Checker as qsc_ast::visit::Visitor}::visit_package
             at ./src/typeck/check.rs:118:13
  41: qsc_frontend::compile::typeck_all
             at ./src/compile.rs:318:5
  42: qsc_frontend::compile::compile
             at ./src/compile.rs:175:28
  43: compile::_::__libfuzzer_sys_run
             at ./fuzz/fuzz_targets/compile.rs:10:17
  44: rust_fuzzer_test_input
             at /home/rokuzmin/.cargo/registry/src/index.crates.io-6f17d22bba15001f/libfuzzer-sys-0.4.6/src/lib.rs:224:17
  45: libfuzzer_sys::test_input_wrap::{{closure}}
             at /home/rokuzmin/.cargo/registry/src/index.crates.io-6f17d22bba15001f/libfuzzer-sys-0.4.6/src/lib.rs:61:9
  46: std::panicking::try::do_call
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/std/src/panicking.rs:485:40
  47: __rust_try
  48: std::panicking::try
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/std/src/panicking.rs:449:19
  49: std::panic::catch_unwind
             at /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/std/src/panic.rs:140:14
  50: LLVMFuzzerTestOneInput
             at /home/rokuzmin/.cargo/registry/src/index.crates.io-6f17d22bba15001f/libfuzzer-sys-0.4.6/src/lib.rs:59:22
  51: _ZN6fuzzer6Fuzzer15ExecuteCallbackEPKhm
             at /home/rokuzmin/.cargo/registry/src/index.crates.io-6f17d22bba15001f/libfuzzer-sys-0.4.6/libfuzzer/FuzzerLoop.cpp:612:13
  52: _ZN6fuzzer10RunOneTestEPNS_6FuzzerEPKcm
             at /home/rokuzmin/.cargo/registry/src/index.crates.io-6f17d22bba15001f/libfuzzer-sys-0.4.6/libfuzzer/FuzzerDriver.cpp:324:6
  53: _ZN6fuzzer12FuzzerDriverEPiPPPcPFiPKhmE
             at /home/rokuzmin/.cargo/registry/src/index.crates.io-6f17d22bba15001f/libfuzzer-sys-0.4.6/libfuzzer/FuzzerDriver.cpp:860:9
  54: main
             at /home/rokuzmin/.cargo/registry/src/index.crates.io-6f17d22bba15001f/libfuzzer-sys-0.4.6/libfuzzer/FuzzerMain.cpp:20:10
  55: __libc_start_main
             at /build/glibc-SzIz7B/glibc-2.31/csu/../csu/libc-start.c:308:16
  56: _start
note: Some details are omitted, run with `RUST_BACKTRACE=full` for a verbose backtrace.
==16693== ERROR: libFuzzer: deadly signal
```

</details>

After the steps above you typically get all the necessary information to file (or to work on) a bug - the short enough input and the panic stack trace.

If the input is still too long then you may want to shorten it manually (e.g. remove the Q# code comments from the Q# input).

If you believe that the input is still longer than sufficient to reproduce the panic, e.g. the panic complains about a local variable in the Q# input,
and in the Q# input you have a dozen of functions with a few dozens of nested scopes with local variables, then you will likely want to break in the debugger
upon panic and see the particular local variable that caused the panic.

To achieve that, you need to rebuild the fuzzing binary with the debugging information (`--dev`):  
`cargo fuzz build --dev compile`.  
The resulting binary "compile" should be in the "debug", not "release", directory

```bash
ls fuzz/target/x86_64-unknown-linux-gnu/debug/
# ... compile ...
```

In your WSL session go to the root directory ("qsharp") of this repo and launch VSCode

```bash
code .
```

(Assuming your VSCode has CodeLLDB extension installed)

- Click the "Run and Debug" view on the left (or press `<Ctrl+Shift+d>`).
- Click the "create a launch.json file" link. Select debugger "LLDB".
- Feel free to reply "No" to the question  
  "Cargo.toml has been detected in the workspace.  
  Would you like to generate launch configurations for its targets?".

<details><summary>Change the contents of "launch.json" to look like this (click this line).</summary>

```json
{
  // Use IntelliSense to learn about possible attributes.
  // Hover to view descriptions of existing attributes.
  // For more information, visit: https://go.microsoft.com/fwlink/?linkid=830387
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug",
      "program": "${workspaceFolder}/fuzz/target/x86_64-unknown-linux-gnu/debug/compile",
      "args": [
        "fuzz/artifacts/compile/minimized-from-b665e6267c297608e85c5948481cd353107a07fa"
      ],
      "cwd": "${workspaceFolder}"
    }
  ]
}
```

</details>

- Press `<F5>` to run the debugging session. The debugger will stop upon panic.
- Look at the call stack, click the stack frames of interest and inspect the local variables and
  parameters in those frames to figure out the exact input fragment that caused the panic.

Then you can manually minimize the input around the fragment of interest.

## Adding More Fuzzing Targets

```bash
cargo fuzz add <new_fuzzing_target_identifier>
cargo fuzz list                 # Optional. See the available fuzzing targets.
# Edit the "fuzz/fuzz_targets/<new_fuzzing_target_identifier>.rs".
cargo fuzz build                # Optional. Build the fuzzing targets.
cargo fuzz run <new_fuzzing_target_identifier>  # Build and run.
# See "Running" section for fine-tuning the runs.
```

## Adding More Seed Inputs for Fuzzing

Add more files with input sequences to the  
"fuzz/seed_inputs/\<fuzzing_target>/" directory and add their paths to the list in  
"fuzz/seed_inputs/\<fuzzing_target>/list.txt".

Details

```bash
cargo fuzz run compile --features do_fuzz -- -help=1 2>&1 | grep seed_inputs
#  seed_inputs      0 A comma-separated list of input files to use as an additional seed corpus.
#                     Alternatively, an "@" followed by the name of a file containing the comma-separated list.
```

See more in [LibFuzzer Corpus](https://llvm.org/docs/LibFuzzer.html#corpus).

## Code Coverage During Fuzzing

Based on [Code Coverage](https://rust-fuzz.github.io/book/cargo-fuzz/coverage.html#code-coverage).

Tested in WSL Ubuntu 22.04.

### Code Coverage Prerequisites

Note: The command `sudo apt install clang` installed `clang-10` and created the executables `clang` and `clang++` available in the `PATH`.
The installation of other versions, like `sudo apt install clang-14` was installing the executables `clang-14` and `clang++-14`,
but the executables `clang` and `clang++` were still of version 10.

For the subsequent steps to succeed the executables `llvm-profdata` and `llvm-cov` need to be in the `PATH`.

```bash
which llvm-profdata<tab><tab>   # See if the `llvm-profdata` is available.
# llvm-profdata-10              # Not available, but version 10 is installed.
which llvm-profdata-10          # See the path of version 10.
# /usr/bin/llvm-profdata-10     # The path of version 10.
pushd /usr/bin                  # Temporarily enter the dir where `llvm-profdata-10` is located.
sudo ln -s llvm-profdata-10 llvm-profdata  # Create symlink `llvm-profdata` -> `llvm-profdata-10`.
sudo ln -s llvm-cov-10      llvm-cov       # Create symlink `llvm-cov`      -> `llvm-cov-10`.
popd                            # Get back to the original dir.

llvm-cov --version              # Optional. See the version.
#LLVM (http://llvm.org/):
#  LLVM version 10.0.0
#  Optimized build.
#  Default target: x86_64-pc-linux-gnu
#  Host CPU: skylake
```

The executables `llvm-profdata` and `llvm-cov` also need to be in the nightly toolchain.

```bash
rustup default                                  # Make sure that the nightly toolchain is the default.
# nightly-x86_64-unknown-linux-gnu (default)

# See if the executables `llvm-profdata` and `llvm-cov` are installed in the nightly toolchain:
ls /home/$USER/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/
# gcc-ld  llc                   # Nothing starting with `llvm-`.

# Not installed, install:
rustup component add --toolchain nightly llvm-tools-preview

# Make sure they are installed:
ls /home/$USER/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/
# gcc-ld  llc  llvm-ar  llvm-as  llvm-cov  llvm-dis  llvm-nm  llvm-objcopy  llvm-objdump  llvm-profdata  llvm-readobj  llvm-size  llvm-strip  opt  rust-lld

# Optional. See the version:
/home/$USER/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-cov --version
#LLVM (http://llvm.org/):
#  LLVM version 16.0.2-rust-1.70.0-nightly
#  Optimized build.
```

Install the Rust demangler.  
`cargo install rustfilt`

### Running the Code Coverage Tool

```bash
# In "qsc_frontend" directory:

# Make sure that fuzzing still works OK:
cargo fuzz list                 # Optional. See the fuzzing targets.
#compile
cargo fuzz run compile --features do_fuzz -- -seed_inputs=@fuzz/seed_inputs/compile/list.txt -max_total_time=1
    # Run the fuzzing for at least 1 second.
    # It is assumed that earlier you were running `cargo fuzz run compile` for a long time to gather
    # the execution statistics.

cargo fuzz coverage compile         # Gather the code coverage info.
                                    # The run takes a few minutes.
# Later you will likely need the following data:
# One of the first log lines shows the absolute path to the executable the code coverage is gathered for:
# .../target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/compile
# The last log line shows the absolute path to the file containing the code coverage info:
# .../fuzz/coverage/compile/coverage.profdata

# Generate the HTML-report showing the code coverage for the fuzzing executable:
/home/$USER/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-cov \
    show -Xdemangler=rustfilt -show-line-counts-or-regions -show-instantiations --ignore-filename-regex="/home/$USER/.cargo/.*" \
    -format=html \
    -instr-profile=fuzz/coverage/compile/coverage.profdata \
    target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/compile \
    > index.html
# The unrelated error and warning that were observed (did not affect the result):
#error: /rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/std/src/sys/common/thread_local/fast_local.rs: No such file or directory
#warning: The file '/rustc/88fb1b922b047981fc0cfc62aa1418b4361ae72e/library/std/src/sys/common/thread_local/fast_local.rs' isn't covered.

# Open the "index.html" in the web-browser to see the code coverage report (not necessarily in WSL).
```
