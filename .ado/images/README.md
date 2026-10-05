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

## Building and using images in ADO

To put LLVM in an ADO feed as a universal package:

- Download from <https://github.com/llvm/llvm-project/archive/refs/tags/llvmorg-20.1.8.tar.gz> (or similar)
- Upload as a Univseral Package via:

```bash
az artifacts universal publish \
  --organization https://dev.azure.com/<org> \
  --project <project> --scope project \
  --feed "azure-quantum" \
  --name llvm-source \
  --version 20.1.8 \
  --path ~/Downloads/llvm-project-llvmorg-20.1.8.tar.gz \
  --description "LLVM source tarball"
```

Then in the pipeline

```yaml
# Download the package
- task: UniversalPackages@1
  displayName: Download LLVM source
  inputs:
    command: download
    feed: "AzureQuantum/azure-quantum"
    packageName: "llvm-source"
    packageVersion: "20.1.8" # or '*', '18.*' (wildcards supported for download)
    directory: "$(Pipeline.Workspace)/llvm-src"

# Copy it into the Docker context
- script: |
    set -euxo pipefail
    ls -lh $(Pipeline.Workspace)/llvm-src
    mkdir -p docker-context/third_party
    cp $(Pipeline.Workspace)/llvm-src/llvm-project-18.1.8.tar.gz docker-context/third_party/
  displayName: Stage LLVM tarball into Docker build context
```

Note that with network isolation the Universal Package download may fail, as it needs to fetch from
blob storage. To enable this in a 1ES template, add the networkIsolationPolicy below:

```yaml
extends:
  template: v1/1ES.Official.PipelineTemplate.yml@1ESPipelineTemplates
  parameters:
    settings:
      # To fetch the Universal Package from the ADO feed, Azure blob access is needed :(
      networkIsolationPolicy: AzureStorage
```

Then unpack it in the Dockerfile to the desired location, e.g.

```yaml
COPY third_party/llvm-project-18.1.8.tar.gz /tmp/
RUN tar -xzf /tmp/llvm-project-18.1.8.tar.gz -C /tmp/llvm-project
```

TODO

- Document how to create the service connections and set permissions
- Document how to use an image in a ADO pipeline
- Document how to create and publish an image in an ADO pipeline

## BONEYARD

The below was too slow to run on ACR, so trying to move it to ADO Hosted Pools and pipelines

```bash
IMAGE_TAG="$(git rev-parse --short HEAD)"
CONTEXT="${REPO}#${BRANCH}:.ado/images/qdk-image"
ACR_LOGIN_SERVER="$(
  az acr show --name "$ACR_NAME" --query loginServer --output tsv
)"
```

```bash
# This build on x64 took 1h15m last run.
IMAGE_TAG="20261004"
az acr build --registry "$ACR_NAME" --platform linux/amd64 --timeout 12000 \
  --build-arg BASE_REGISTRY="$ACR_LOGIN_SERVER" \
  --image "qdk-image:${IMAGE_TAG}-amd64" \
  "$CONTEXT"

# Max timeout is 28,800 (8 hours). Building LLVM on QEMU simulated ARM64 is slooooow
az acr build --registry "$ACR_NAME" --platform linux/arm64 --timeout 28800 \
  --build-arg BASE_REGISTRY="$ACR_LOGIN_SERVER" \
  --image "qdk-image:${IMAGE_TAG}-arm64" \
  "$CONTEXT"



# Create the multi-platform manifest for an image for the form: <name>:<tag>-<arch>
# Ensure Docker is logged in
az acr login --name "$ACR_NAME"
IMAGE_NAME=qdk-rust
IMAGE_TAG=20261005
docker buildx imagetools create \
    --tag "$ACR_LOGIN_SERVER/${IMAGE_NAME}:${IMAGE_TAG}" \
    "$ACR_LOGIN_SERVER/${IMAGE_NAME}:${IMAGE_TAG}-amd64" \
    "$ACR_LOGIN_SERVER/${IMAGE_NAME}:${IMAGE_TAG}-arm64"

# Point 'latest' to that if desired
docker buildx imagetools create \
    --tag "$ACR_LOGIN_SERVER/${IMAGE_NAME}:latest" \
    "$ACR_LOGIN_SERVER/${IMAGE_NAME}:${IMAGE_TAG}"

# Get it locally, add a friendly tag, and run it
docker pull "$ACR_LOGIN_SERVER/${IMAGE_NAME}"
docker tag "$ACR_LOGIN_SERVER/${IMAGE_NAME}" ${IMAGE_NAME}
docker run --rm -it ${IMAGE_NAME}

# To run as a non-root user, add: --user 1000:1000
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
