"""Image update decisions and matching agent/proxy pairs."""

from __future__ import annotations

import base64
from dataclasses import dataclass, field
import json
import logging
import os
from pathlib import Path
import re
import shutil
from subprocess import CompletedProcess
import tempfile

from .common import (
    LauncherError,
    command_error,
    format_error,
    register_secret,
    run,
)
from .podman import INTERFACE_LABEL, INTERFACE_VERSION, normalize_image_id, podman

REGISTRY = "qdkacr-cfgqhwddajdaebhg.azurecr.io"
LOCAL_IMAGE_PREFIX = "copilot-sandbox-"
LOCAL_IMAGE_TAG = "local"
BUILD_ID_LABEL = "qdk.copilot-sandbox.build-id"
LOGGER = logging.getLogger(__name__)


@dataclass(frozen=True)
class _RemoteImage:
    reference: str


class _RegistryError(LauncherError):
    def __init__(
        self, image: str, result: CompletedProcess[str], operation: str
    ) -> None:
        diagnostic = result.stderr or ""
        self.authentication_failed = bool(
            re.search(
                r"\b(?:unauthorized|authentication required|authentication failed|"
                r"authorization failed|access denied)\b|\b(?:HTTP|status(?: code)?)\s*:?\s*(?:401|403)\b",
                diagnostic,
                re.IGNORECASE,
            )
        )
        if re.search(
            r"no such manifest|manifest unknown|manifest_unknown|name_unknown",
            diagnostic,
            re.IGNORECASE,
        ):
            message = "Registry image was not found."
            hint = "Verify that the requested image tag was published."
            if image.endswith(":latest"):
                hint = (
                    "Use --tag BUILD_ID; feature-branch builds do not publish latest."
                )
        elif self.authentication_failed:
            message = "Registry authentication or authorization failed."
            hint = "Check ACR pull permissions and registry login."
        else:
            message = "Registry operation failed."
            hint = "Check registry connectivity and availability."
        super().__init__(
            f"{message} {hint}\n"
            + str(command_error(result, operation=f"podman {operation} {image}"))
        )


def _request_user_confirmation(prompt: str) -> bool:
    return input(prompt + " [y/N]: ").strip().lower() == "y"


@dataclass
class _RegistryAuth:
    """Registry credentials for one launch: an ACR token in a temporary file, never saved by Podman."""

    directory: Path
    arguments: list[str] = field(default_factory=list)

    def run(self, operation: list[str], reference: str) -> CompletedProcess[str]:
        result = run(podman(*operation, *self.arguments, reference), check=False)
        if result.returncode:
            error = _RegistryError(reference, result, " ".join(operation))
            if self.arguments or not error.authentication_failed:
                raise error
            LOGGER.warning("%s", format_error(error))
            self._write_token()
            result = run(podman(*operation, *self.arguments, reference), check=False)
            if result.returncode:
                raise _RegistryError(reference, result, " ".join(operation))
        return result

    def _write_token(self) -> None:
        executable = shutil.which("az")
        if executable is None:
            raise LauncherError("Azure CLI (az) is not installed or available on PATH.")
        print("Requesting a temporary ACR token with az.")
        result = run(
            [
                executable,
                "acr",
                "login",
                "--name",
                "qdkacr",
                "--expose-token",
                "--output",
                "json",
            ]
        )
        try:
            data = json.loads(result.stdout)
            token, server = data["accessToken"], data["loginServer"]
        except (ValueError, KeyError, TypeError) as error:
            raise LauncherError("Unexpected output from az acr login.") from error
        register_secret(token)
        if server != REGISTRY:
            raise LauncherError(
                f"az acr login returned an unexpected registry: {server}"
            )
        credential = base64.b64encode(
            b"00000000-0000-0000-0000-000000000000:" + token.encode()
        ).decode()
        register_secret(credential)
        path = self.directory / "auth.json"
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as stream:
            json.dump({"auths": {server: {"auth": credential}}}, stream)
        self.arguments = ["--authfile", str(path)]


def get_image_names_for_tag(tag: str, *, local: bool = False) -> tuple[str, str]:
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}", tag):
        raise LauncherError("Invalid image tag.")
    prefix = LOCAL_IMAGE_PREFIX if local else f"{REGISTRY}/copilot-sandbox/"
    return f"{prefix}agent:{tag}", f"{prefix}proxy:{tag}"


def _get_local_image_metadata(image: str) -> dict | None:
    result = run(podman("image", "inspect", image), check=False)
    if result.returncode:
        if re.search(
            r"image not known|no such image|no such object",
            result.stderr,
            re.IGNORECASE,
        ):
            return None
        raise command_error(result, operation=f"Inspect local image {image}")
    try:
        metadata = json.loads(result.stdout)[0]
        if not isinstance(metadata, dict):
            raise ValueError("Expected an image metadata object.")
        metadata["Id"] = normalize_image_id(metadata["Id"])
        return metadata
    except (ValueError, KeyError, IndexError, TypeError) as error:
        raise LauncherError(
            f"Local image metadata has an unexpected shape: {image}"
        ) from error


def _get_build_id(image: dict) -> str:
    labels = (image.get("Config") or {}).get("Labels") or {}
    build_id = labels.get(BUILD_ID_LABEL)
    if not isinstance(build_id, str) or not build_id:
        raise LauncherError(
            "Image has no sandbox build-id label; rebuild or select a paired release."
        )
    if labels.get(INTERFACE_LABEL) != INTERFACE_VERSION:
        raise LauncherError(
            f"Image build {build_id} predates the Podman sandbox; "
            "rebuild with build_images.py or select a newer build."
        )
    get_image_names_for_tag(build_id)
    return build_id


def _get_remote_image(
    image: str, architecture: str, auth: _RegistryAuth
) -> _RemoteImage:
    result = auth.run(["manifest", "inspect"], image)
    try:
        index = json.loads(result.stdout)
        digests = [
            entry["digest"]
            for entry in index["manifests"]
            if entry.get("platform", {}).get("architecture") == architecture
            and entry.get("platform", {}).get("os") == "linux"
        ]
        if len(digests) != 1:
            raise LauncherError(
                f"Registry image {image} has no single linux/{architecture} manifest."
            )
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", digests[0]):
            raise ValueError("Invalid image digest.")
        # Pulling by digest is content-addressed. Podman can't inspect the per-platform manifest:
        # 5.3+ drops its config, and OCI image manifests are refused.
        return _RemoteImage(image.rsplit(":", 1)[0] + "@" + digests[0])
    except (ValueError, KeyError, TypeError, AttributeError) as error:
        raise LauncherError(
            f"Registry image metadata has an unexpected shape: {image}"
        ) from error


def _get_cached_image_pair(base: str, *, local: bool = False) -> tuple[str, str, str]:
    agent = _get_local_image_metadata(base)
    if agent is None:
        raise LauncherError(f"No local agent image is available as {base}.")
    build_id = _get_build_id(agent)
    _, proxy = get_image_names_for_tag(build_id, local=local)
    proxy_image = _get_local_image_metadata(proxy)
    if proxy_image is None:
        raise LauncherError(
            f"Cached agent build {build_id} requires missing proxy image {proxy}."
        )
    if _get_build_id(proxy_image) != build_id:
        raise LauncherError(
            "Agent and proxy build-id labels differ; refusing a mixed-build session."
        )
    return agent["Id"], proxy_image["Id"], build_id


def get_reused_image_pair(
    config: dict, architecture: str, *, instance: str
) -> tuple[str, str, str]:
    if not any(key in config for key in ("agent_id", "proxy_id", "build_id")):
        raise LauncherError(
            f"No previous image pair is recorded for instance '{instance}'. "
            "Launch it without --reuse first."
        )
    for key in ("agent_id", "proxy_id"):
        value = config.get(key)
        if not isinstance(value, str) or not re.fullmatch(
            r"sha256:[0-9a-f]{64}", value
        ):
            raise LauncherError(
                f"Invalid recorded image ID '{key}' for instance '{instance}'."
            )
    build_id = config.get("build_id")
    if not isinstance(build_id, str):
        raise LauncherError(f"Invalid recorded build ID for instance '{instance}'.")
    get_image_names_for_tag(build_id)
    for key in ("agent_id", "proxy_id"):
        image_id = config[key]
        metadata = _get_local_image_metadata(image_id)
        if metadata is None:
            raise LauncherError(
                f"Cannot reuse instance '{instance}': {key} {image_id} is missing locally. "
                "Launch without --reuse to select another image pair."
            )
        if metadata.get("Id") != image_id:
            raise LauncherError(
                f"Recorded image identity does not match {key}: {image_id}"
            )
        if (
            metadata.get("Os") != "linux"
            or metadata.get("Architecture") != architecture
        ):
            raise LauncherError(
                f"Recorded image {key} is not linux/{architecture}: {image_id}"
            )
        if _get_build_id(metadata) != build_id:
            raise LauncherError(
                f"Recorded image {key} does not match build {build_id}: {image_id}"
            )
    return config["agent_id"], config["proxy_id"], build_id


def _pull_image(image: _RemoteImage, auth: _RegistryAuth) -> dict:
    metadata = _get_local_image_metadata(image.reference)
    if metadata is None:
        auth.run(["pull", "--quiet"], image.reference)
        metadata = _get_local_image_metadata(image.reference)
        if metadata is None:
            raise LauncherError(
                f"Downloaded image is missing locally: {image.reference}"
            )
    if image.reference not in (metadata.get("RepoDigests") or []):
        raise LauncherError(
            f"Downloaded image does not carry the expected digest: {image.reference}"
        )
    return metadata


def _get_cached_fallback(base: str) -> tuple[str, str, str]:
    try:
        cached = _get_cached_image_pair(base)
    except LauncherError as cache_error:
        raise LauncherError(
            "Cannot select sandbox images. Cached fallback failed."
        ) from cache_error
    if _request_user_confirmation(
        "Cannot compare registry digests. Continue with the cached image pair?"
    ):
        return cached
    raise LauncherError("Use of the cached image pair was declined.") from None


def get_image_pair(
    base: str, architecture: str, *, local: bool = False
) -> tuple[str, str, str]:
    if local:
        return _get_cached_image_pair(base, local=True)
    with tempfile.TemporaryDirectory(prefix="qdk-sandbox-auth-") as directory:
        auth = _RegistryAuth(Path(directory))
        try:
            agent = _get_remote_image(base, architecture, auth)
        except LauncherError as error:
            LOGGER.warning("%s", format_error(error))
            return _get_cached_fallback(base)
        # Podman can't read a remote image's labels, so the paired proxy is known only once the agent is local.
        proxy = None
        missing = f"  {agent.reference}\n  (and its paired proxy)"
        local_agent = _get_local_image_metadata(agent.reference)
        if local_agent is not None:
            _, proxy_name = get_image_names_for_tag(_get_build_id(local_agent))
            try:
                proxy = _get_remote_image(proxy_name, architecture, auth)
            except LauncherError as error:
                LOGGER.warning("%s", format_error(error))
                return _get_cached_fallback(base)
            missing = (
                ""
                if _get_local_image_metadata(proxy.reference)
                else f"  {proxy.reference}"
            )
        if missing and not _request_user_confirmation(
            f"Pull the latest sandbox images?\n{missing}"
        ):
            try:
                return _get_cached_image_pair(base)
            except LauncherError as error:
                raise LauncherError(
                    "Downloads declined and no matching cached image pair is available."
                ) from error
        try:
            agent_metadata = _pull_image(agent, auth)
            build_id = _get_build_id(agent_metadata)
            _, proxy_name = get_image_names_for_tag(build_id)
            proxy_metadata = _pull_image(
                proxy or _get_remote_image(proxy_name, architecture, auth), auth
            )
        except _RegistryError as error:
            raise LauncherError(
                "Cannot pull required sandbox images; see diagnostics above."
            ) from error
    if _get_build_id(proxy_metadata) != build_id:
        raise LauncherError(
            "Agent and proxy build-id labels differ; refusing a mixed-build session."
        )
    for name, metadata in ((base, agent_metadata), (proxy_name, proxy_metadata)):
        tagged = _get_local_image_metadata(name)
        if tagged is None or tagged.get("Id") != metadata["Id"]:
            run(podman("tag", metadata["Id"], name))
    return agent_metadata["Id"], proxy_metadata["Id"], build_id
