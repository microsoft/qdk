# xcode-runner

This project exists to easily launch the `gpu-runner` binary under Xcode for shader debugging purposes. If you are able to reproduce an issue with the GPU full state simulator, write code in the `gpu-runner` binary to trigger the issue, then use this project to launch the simulator under Xcode and use its GPU debugging tools.

## Why?

Debugging GPU shaders can be difficult. Xcode provides a convenient way to launch and debug GPU applications on macOS. This is a lot easier than trying to attach to a running process or figure out how to launch a specific Rust unit tests from the command line.

See <https://developer.apple.com/documentation/xcode/metal-debugger> for more information on debugging GPU shaders with Xcode.
