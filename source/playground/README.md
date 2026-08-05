# Playground

This is a simple web site built using the Monaco editor and the qsharp npm package. A live version is hosted at https://microsoft.github.io/qdk/, and requires no installation.

## Building the Playground Locally

1. Build the entire repo by running `./build.py` in the root directory.
   If you only want to build the functionality necessary to run the playground, you can use `python ./build.py --wasm --npm --play`.
2. Then make `./playground` your current directory and run `npm start` to start the web server.
3. Copy the URL that will be printed to console and open it in a browser to use the playground.

## Building the Playground Locally in Watch Mode

1. Build the entire repo by running `./build.py` in the root directory.
   If you only want to build the functionality necessary to run the playground, you can use `python ./build.py --wasm --npm --play`.
2. Run `node watch.mjs` at the root of the repo.
3. Copy the URL that will be printed to the console and open it in a browser to use the playground.

Any time you make changes to any of the source files, the changes will be automatically compiled. Refresh the page in the browser to reload the playground with your changes.
