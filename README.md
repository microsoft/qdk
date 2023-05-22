# Q# language and tooling

> Under development

This repository contains Q# tooling, specifically:

- **[compiler](./compiler/qsc/)**: command line compiler
- **[library](./library/)**: standard library
- **[npm](./npm/)**: Q# npm module
- **[pip](./pip/)**: Q# Python pip package
- **[playground](./playground/)**: simple website for interacting with Q#
- **[vscode](./vscode/)**: Visual Studio Code extension

There are also the learning 'katas' and Q# samples in the `./katas` and `./samples`
directories, respectively.

## Building

To build this repository there are 4 dependencies that need to be installed. These are:

- Python (<https://python.org>)
- Rust (<https://www.rust-lang.org/tools/install>)
- Node.js (<https://nodejs.org/>)
- wasm-pack (<https://rustwasm.github.io/wasm-pack/installer/>)

The build script will check these dependencies and their versions and fail if not met. (Or run
`python ./prereqs.py` directly to check if the minimum required versions are installed).

To build, in the root directory run `python ./build.py`. By default this will run a release
build of each project, including running tests and checks such as linting. Run with the
`--help` option for detailed usage.

### Playground

To run the "playground" locally, `cd` into the `playground` directory, and run `npm start`.
This will launch a local web server and output the URL to visit to the console.

### Python

If the build script does not detect a current Python virtual environment, it will automatically create
one under `pip/.venv` to run the tests for the `pip` package. When developing locally,
you can use this virtual environment to run the tests by running `source pip/.venv/bin/activate`
(Linux/MacOS) or `pip/.venv/Scripts/activate.bat` (Windows).

## Code editing

The easiest way to develop in this repo is to use VS Code. When you open the project root, by
default VS Code will recommend you install the extensions listed in `.vscode/extensions.json`.
These extensions provide language services for editing, as well as linters and formatters to
ensure the code meets the requirements (which are checked by the `build.py` script and CI).

Some settings are recommended (but not enforced) to make development easier. These are in the
`.vscode/*.shared.json` files. If the [Workspace Config+](https://marketplace.visualstudio.com/items?itemName=swellaby.workspace-config-plus)
extension is installed, this will automatically apply these settings, as well as overrides from
your own corresponding `.vscode/*.local.json` settings. If you don't install this extension, you can
use these as a reference for editing your own `.vscode/*.json` settings files. (See the extension
home page for more details).

## Feedback

If you have feedback about the content in this repository, please let us know by filing a [new issue](https://github.com/microsoft/qsharp/issues/new/choose)!

## Reporting Security Issues

Security issues and bugs should be reported privately following our [security issue documentation](./SECURITY.md#reporting-security-issues).

## Contributing

This project welcomes contributions and suggestions. Most contributions require you to agree to a
Contributor License Agreement (CLA) declaring that you have the right to, and actually do, grant us
the rights to use your contribution. For details, visit <https://cla.opensource.microsoft.com>.

When you submit a pull request, a CLA bot will automatically determine whether you need to provide
a CLA and decorate the PR appropriately (e.g., status check, comment). Simply follow the instructions
provided by the bot. You will only need to do this once across all repos using our CLA.

This project has adopted the [Microsoft Open Source Code of Conduct](https://opensource.microsoft.com/codeofconduct/).
For more information see the [Code of Conduct FAQ](https://opensource.microsoft.com/codeofconduct/faq/) or
contact [opencode@microsoft.com](mailto:opencode@microsoft.com) with any additional questions or comments.

For more details, please see [CONTRIBUTING.md](./CONTRIBUTING.md).

## Legal and Licensing

### Trademarks

This project may contain trademarks or logos for projects, products, or services. Authorized use of Microsoft
trademarks or logos is subject to and must follow
[Microsoft's Trademark & Brand Guidelines](https://www.microsoft.com/en-us/legal/intellectualproperty/trademarks/usage/general).
Use of Microsoft trademarks or logos in modified versions of this project must not cause confusion
or imply Microsoft sponsorship. Any use of third-party trademarks or logos are subject to those
third-party's policies.
