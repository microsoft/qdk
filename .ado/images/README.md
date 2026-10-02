# Images for manylinux_2_28

## Using ACR

Assuming the below has been run:

```bash
ACR_NAME=qdkacr

# Where to run the ACR tasks from
REPO=https://github.com/microsoft/qdk.git
BRANCH="billti/manylinux_2_28"

# Import the upstream images
UBI8_UPSTREAM=registry.access.redhat.com/ubi8/ubi:8.10-1790754002
az acr import --name $ACR_NAME --source $UBI8_UPSTREAM --image ubi8/ubi:8.10
```

Use `az acr build` to queue separate AMD64 and ARM64 builds. ACR executes ARM builds
through QEMU on an AMD64 worker; the `--platform` argument selects the target image
platform rather than a native ARM64 worker. Using `az acr build` applies that target
directly to the Docker build. Use the same image tag for both builds:

```bash
IMAGE_TAG="$(git rev-parse --short HEAD)"
CONTEXT="${REPO}#${BRANCH}:.ado/images/qdk-image"
ACR_LOGIN_SERVER="$(
  az acr show --name "$ACR_NAME" --query loginServer --output tsv
)"

AMD64_RUN_ID=$(
  az acr build --registry "$ACR_NAME" --platform linux/amd64 --timeout 12000 \
    --build-arg BASE_REGISTRY="$ACR_LOGIN_SERVER" \
    --build-arg EXPECTED_MACHINE=x86_64 \
    --image "qdk-image:${IMAGE_TAG}-amd64" \
    --no-wait "$CONTEXT" --query runId --output tsv
)

ARM64_RUN_ID=$(
  az acr build --registry "$ACR_NAME" --platform linux/arm64 --timeout 12000 \
    --build-arg BASE_REGISTRY="$ACR_LOGIN_SERVER" \
    --build-arg EXPECTED_MACHINE=aarch64 \
    --image "qdk-image:${IMAGE_TAG}-arm64" \
    --no-wait "$CONTEXT" --query runId --output tsv
)

printf 'AMD64 run: %s\nARM64 run: %s\n' "$AMD64_RUN_ID" "$ARM64_RUN_ID"
```

The Dockerfile checks the architecture before performing the expensive package and LLVM
build steps. After both runs report `Succeeded`, create the versioned and `latest`
multi-architecture manifests:

```bash
az acr run --registry "$ACR_NAME" --timeout 1800 \
  --set imageTag="$IMAGE_TAG" \
  --file manifest-task.yaml "$CONTEXT"
```

To view and check on runs:

```bash
# Show recent runs
az acr task list-runs --registry "$ACR_NAME" --top 10 --output table

# See the status of a run
az acr task show-run --registry "$ACR_NAME" --run-id "$RUN_ID" --output table

# Reconnect the logs for a run
az acr task logs --registry "$ACR_NAME" --run-id "$RUN_ID"
```

## Building within the container images

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
