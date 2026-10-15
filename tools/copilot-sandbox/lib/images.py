"""Image update decisions and matching agent/proxy pairs."""

from __future__ import annotations

import json
import re

from .common import LauncherError, run

REGISTRY = "qdkacr-cfgqhwddajdaebhg.azurecr.io"
LOCAL_IMAGE_PREFIX = "copilot-sandbox-"
LOCAL_IMAGE_TAG = "dev"
BUILD_ID_LABEL = "qdk.copilot-sandbox.build-id"


def _request_user_confirmation(prompt: str) -> bool:
    return input(prompt + " [y/N]: ").strip().lower() == "y"


def get_image_names_for_tag(tag: str, *, dev: bool = False) -> tuple[str, str]:
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}", tag):
        raise LauncherError("Invalid image tag.")
    prefix = LOCAL_IMAGE_PREFIX if dev else f"{REGISTRY}/copilot-sandbox/"
    return f"{prefix}agent:{tag}", f"{prefix}proxy:{tag}"


def _get_local_image_metadata(image: str) -> dict | None:
    result = run(["docker", "image", "inspect", image], check=False)
    return json.loads(result.stdout)[0] if result.returncode == 0 else None


def _is_local_image_in_manifest(
    local: dict, manifest: object, architecture: str
) -> bool:
    entries = manifest if isinstance(manifest, list) else [manifest]
    for entry in entries:
        if not isinstance(entry, dict):
            raise LauncherError("Registry manifest has an unexpected shape.")
        descriptor = entry.get("Descriptor", {})
        platform = descriptor.get("platform", {})
        if (
            platform.get("architecture", architecture) != architecture
            or platform.get("os", "linux") != "linux"
        ):
            continue
        if any(
            item.endswith("@" + descriptor.get("digest", "missing"))
            for item in local.get("RepoDigests", [])
        ):
            return True
        body = entry.get("SchemaV2Manifest", entry.get("OCIManifest", {}))
        if body.get("config", {}).get("digest") == local.get("Id"):
            return True
    return False


# Pull the image if we don't already have it
def _ensure_image(image: str, architecture: str, *, dev: bool = False) -> str:
    local = _get_local_image_metadata(image)
    if dev:
        if not local:
            raise LauncherError(
                "Development image is missing. Build it with the maintainer tool first."
            )
        return local["Id"]
    remote = run(["docker", "manifest", "inspect", "--verbose", image], check=False)
    if remote.returncode:
        if _request_user_confirmation(
            "Registry inspection failed. Run az acr login --name qdkacr and retry?"
        ):
            run(["az", "acr", "login", "--name", "qdkacr"])
            remote = run(
                ["docker", "manifest", "inspect", "--verbose", image], check=False
            )
        if remote.returncode:
            if local and _request_user_confirmation(
                "Cannot compare registry digests. Continue with the cached image?"
            ):
                return local["Id"]
            raise LauncherError(
                "Cannot inspect the registry image; verify ACR access and connectivity."
            )
    changed = local is None or not _is_local_image_in_manifest(
        local, json.loads(remote.stdout), architecture
    )
    if changed and _request_user_confirmation(
        f"{'Pull missing' if local is None else 'Pull updated'} image {image}?"
    ):
        pulled = run(["docker", "pull", image], check=False)
        if pulled.returncode and _request_user_confirmation(
            "Pull failed. Run az acr login --name qdkacr and retry?"
        ):
            run(["az", "acr", "login", "--name", "qdkacr"])
            run(["docker", "pull", image])
        elif pulled.returncode:
            raise LauncherError("Image pull failed.")
        local = _get_local_image_metadata(image)
    if local is None:
        raise LauncherError(
            "A local image is required; pull was declined or unsuccessful."
        )
    return local["Id"]


def get_image_pair(
    base: str, architecture: str, *, dev: bool = False
) -> tuple[str, str, str]:
    agent_id = _ensure_image(base, architecture, dev=dev)
    agent = _get_local_image_metadata(agent_id) or {}
    build_id = ((agent.get("Config") or {}).get("Labels") or {}).get(BUILD_ID_LABEL)
    if not isinstance(build_id, str) or not build_id:
        raise LauncherError(
            "Agent image has no sandbox build-id label; rebuild or select a paired release."
        )
    _, proxy = get_image_names_for_tag(build_id, dev=dev)
    proxy_id = _ensure_image(proxy, architecture, dev=dev)
    proxy_image = _get_local_image_metadata(proxy_id) or {}
    proxy_build = ((proxy_image.get("Config") or {}).get("Labels") or {}).get(
        BUILD_ID_LABEL
    )
    if proxy_build != build_id:
        raise LauncherError(
            "Agent and proxy build-id labels differ; refusing a mixed-build session."
        )
    return agent_id, proxy_id, build_id
