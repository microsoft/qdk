# Images for manylinux_2_28

For building and testing PyQIR and the QDK for manylinux_2_28, you can use the containers
defined in this directory.

The first image in the `./qdk-image` directory builds on the
[RHEL 8 base image](https://catalog.redhat.com/en/software/base-images),
installs the necessary `dnf` OS packages (such as GCC and Python), and then builds LLVM 20.
This image should be very stable and rarely need updating.

The second image in the `./qdk-tools` directory builds on the above image, and adds tools
needed to build QDK, such as Rust, Node.js, wasm-bindgen, Maturin, and vsce. This image
should be updated when build tool versions change, and should rebuild relatively quickly.

See the comments at the top of each Dockerfile, then to run an image with an interactive
shell run something like `docker run --name qdk-build -it -v "$PWD:/artifacts" qdk-tools:arm64`

Inside the shell, commands such as those shown below can be used to build and test the
PyQIR and QDK projects, and create the manylinux_2_28 wheels.

As Node.js, the wasm tools, vsce, etc. have been installed, this image can also build the
rest of the QDK artifacts without the need to install any additional tools.

```bash
# Build PyQIR
git clone --depth=1 https://github.com/qir-alliance/pyqir /work/pyqir
export QIRLIB_CACHE_DIR=$LLVM_SYS_201_PREFIX
cd /work/pyqir/qirlib
cargo test  --release --features llvm20-1
cargo build --release --features llvm20-1
cd /work/pyqir/pyqir
python -m build -w \
  --config-setting=build-args="--features llvm20-1 --compatibility"

# Verify the wheel and run the tests
pip install /work/pyqir/target/wheels/*.whl pytest antlr4-python3-runtime==4.11.1
pytest
auditwheel show /work/pyqir/target/wheels/*.whl
cp -v /work/pyqir/target/wheels/*.whl /artifacts/


# Build the QDK
git clone --depth 1 https://github.com/microsoft/qdk /work/qdk && cd /work/qdk

# Ensure it can find the pyqir wheels for testing (if built above)
export PIP_FIND_LINKS=/work/pyqir/target/wheels

BUILD_NUMBER=5 BUILD_TYPE=dev ./version.py
./build.py --qdk --no-check # Can also run --wasm --npm --vscode etc.
auditwheel show /work/qdk/target/wheels/*.whl
cp -v /work/qdk/target/wheels/*.whl /artifacts/
```
