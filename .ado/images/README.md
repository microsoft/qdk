# Images for manylinux_2_28

## Building within the container images

For building and testing PyQIR and the QDK for manylinux_2_28, you can use the containers
defined in this directory.

The first image in the `./qdk-base` directory builds on the
[UBI8 base image](https://developers.redhat.com/articles/ubi-faq) and installs the necessary `dnf` OS packages.

The second image in the `./qdk-build` directory builds on the above image, and adds tools
needed to build PyQIR and the QDK, such as LLVM 20, Rust, and Python 3.15. This image
should be updated when build tool versions change.

## Using ACR

Assuming the below has been run:

```bash
# Azure details
AZ_TENANT_ID="72f9..."
AZ_SUBSCRIPTION_ID="677f..."

# ACR details
ACR_NAME=qdkacr
ACR_LOGIN_SERVER="qdkacr-cfgqhwddajdaebhg.azurecr.io"

# ADO details
ADO_FEED="az..."
ADO_ORG="ms..."
ADO_PROJECT="Az..."

# Upstream UBI8 details
# See https://catalog.redhat.com/en/software/containers/ubi8/5c647760bed8bd28d0e38f9f for the latest UBI8 image tag
UBI_REGISTRY="registry.access.redhat.com"
UBI_TAG="8.10-1793091705"

# Log in to Azure for the right account and subscription
az login --tenant AZ_TENANT_ID --skip-subscription-discovery --subscription AZ_SUBSCRIPTION_ID

# If using images with Docker locally, log Docker into the ACR registry
# Note: Docker Desktop should be running before invoking the below command
az acr login --name "$ACR_NAME"
```

Then import an upstream UBI8 image into the Azure Container Registry

```bash
# To import a base UBI8 image from RedHat
az acr import --name ${ACR_NAME} --source ${UBI_REGISTRY}/ubi8/ubi:${UBI_TAG} --image ubi8/ubi:${UBI_TAG}
# To find the sha256 for the image
docker buildx imagetools inspect "${ACR_LOGIN_SERVER}/ubi8/ubi:${UBI_TAG}"
# The first few lines will contain something like: Digest: sha256:b76f9ead999002af783d7b7036b7b57ceee6033712ad07577240ee06489a3a40
# Use the (immutable) sha256 rather than the (mutable) tag in the Dockerfile that references the image
```

To rebuild the base OS image, update the sha256 in `./qdk-base/Dockerfile` and run:

```bash
# Create the `base` image (UBI8 + dnf packages) with the specified tag
ACR_IMAGE_TAG="20261007"
GH_BRANCH="billti/manylinux_2_28"
az acr run --registry "$ACR_NAME" \
  --set ubi8Tag="$UBI_TAG"
  --set imageTag="$ACR_IMAGE_TAG" \
  --file manifest-task.yaml \
  "https://github.com/microsoft/qdk.git#${GH_BRANCH}:.ado/images/qdk-base"
```

Use the `build-images.yml` ADO pipeline to create the `qdk-build` image. This requires that
a "Docker Registry" service connection has been set up in ADO that has Pull and Push permissions
to the Azure Container Registry.

## Updating artifacts

Now and again the artifacts used within the `qdk-build` image will need updating. For example, let says you want to update
Python in the image to 3.15.1. Follow these steps:

- Locate the artifact(s) to be uploaded. For example, visit <https://www.python.org/downloads/release/python-3151/> and
  download the "XZ compressed source tarball" and make note of the SHA-256 checksum.
- Verify the downloaded package and checksum match, e.g. `sha256sum ~/Downloads/Python-3.15.1.tar.xz`
- Upload the artifact to the feed with the correct name and version, e.g.

```bash
az artifacts universal publish \
  --organization https://dev.azure.com/${ADO_ORG} \
  --project "${ADO_PROJECT}" --scope project \
  --feed "${ADO_FEED}" \
  --name python-source \
  --version "3.15.1" \
  --path ~/Downloads/Python-3.15.1.tar.xz \
  --description "Python source tarball"
```

- Update the pipeline and Dockerfile as needed, e.g. for the above:
  - In `build-images.yml` the task to "Download Python 3.15 source" had the "packageVersion" updated to "3.15.1"
  - In `qdk-build/Dockerfile` the filename in `ARG PY315_ARCHIVE=Python-3.15.0.tar.xz` was updated to `Python-3.15.1.tar.xz` and
    the sha256 value piped into the command `| sha256sum -c -` was updated to match that verified above.
