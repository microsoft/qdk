"""Image update decisions and matching agent/proxy pairs."""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
import json
import re

from .common import LauncherError, run

REGISTRY = "qdkacr-cfgqhwddajdaebhg.azurecr.io"
LOCAL_IMAGE_PREFIX = "copilot-sandbox-"
LOCAL_IMAGE_TAG = "dev"
BUILD_ID_LABEL = "qdk.copilot-sandbox.build-id"


@dataclass(frozen=True)
class _RemoteImage:
    reference: str
    image_id: str
    build_id: str


class _RegistryError(LauncherError):
    pass


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


def _get_build_id(image: dict) -> str:
    build_id = ((image.get("Config") or {}).get("Labels") or {}).get(BUILD_ID_LABEL)
    if not isinstance(build_id, str) or not build_id:
        raise LauncherError(
            "Image has no sandbox build-id label; rebuild or select a paired release."
        )
    get_image_names_for_tag(build_id)
    return build_id


def _get_remote_image_metadata(image: str, architecture: str) -> _RemoteImage:
    result = run(["docker", "manifest", "inspect", "--verbose", image], check=False)
    if result.returncode:
        raise _RegistryError("Cannot inspect the registry image.")
    try:
        manifest = json.loads(result.stdout)
        entries = manifest if isinstance(manifest, list) else [manifest]
        for entry in entries:
            descriptor = entry["Descriptor"]
            platform = descriptor.get("platform", {})
            if (
                platform.get("architecture", architecture) != architecture
                or platform.get("os", "linux") != "linux"
            ):
                continue
            digest = descriptor["digest"]
            body = entry.get("SchemaV2Manifest", entry.get("OCIManifest", {}))
            image_id = body["config"]["digest"]
            if not all(
                re.fullmatch(r"sha256:[0-9a-f]{64}", value)
                for value in (digest, image_id)
            ):
                raise ValueError("Invalid image digest.")
            reference = image.rsplit(":", 1)[0] + "@" + digest
            configuration = run(
                [
                    "docker",
                    "buildx",
                    "imagetools",
                    "inspect",
                    reference,
                    "--format",
                    "{{json .Image}}",
                ],
                check=False,
            )
            if configuration.returncode:
                raise _RegistryError("Cannot inspect the registry image configuration.")
            data = json.loads(configuration.stdout)
            candidates = [data] if "architecture" in data else data.values()
            matches = [
                candidate
                for candidate in candidates
                if isinstance(candidate, dict)
                and candidate.get("architecture") == architecture
                and candidate.get("os") == "linux"
            ]
            if len(matches) != 1:
                raise ValueError("Unexpected image platform.")
            build_id = _get_build_id({"Config": matches[0].get("config")})
            return _RemoteImage(reference, image_id, build_id)
    except (ValueError, KeyError, TypeError, AttributeError):
        raise LauncherError(
            "Registry image metadata has an unexpected shape."
        ) from None
    raise LauncherError(f"Registry image has no linux/{architecture} manifest.")


def _get_cached_image_pair(base: str, *, dev: bool = False) -> tuple[str, str, str]:
    agent = _get_local_image_metadata(base)
    if agent is None:
        raise LauncherError("A local agent image is required; build or pull it first.")
    build_id = _get_build_id(agent)
    _, proxy = get_image_names_for_tag(build_id, dev=dev)
    proxy_image = _get_local_image_metadata(proxy)
    if proxy_image is None:
        raise LauncherError("The cached agent's matching proxy image is missing.")
    if _get_build_id(proxy_image) != build_id:
        raise LauncherError(
            "Agent and proxy build-id labels differ; refusing a mixed-build session."
        )
    return agent["Id"], proxy_image["Id"], build_id


def _get_remote_image_pair(
    base: str, architecture: str
) -> tuple[_RemoteImage, _RemoteImage]:
    agent = _get_remote_image_metadata(base, architecture)
    _, proxy_name = get_image_names_for_tag(agent.build_id)
    proxy = _get_remote_image_metadata(proxy_name, architecture)
    if proxy.build_id != agent.build_id:
        raise LauncherError(
            "Agent and proxy build-id labels differ; refusing a mixed-build session."
        )
    return agent, proxy


def _pull_images(remote_images: list[_RemoteImage]) -> list[_RemoteImage]:
    with ThreadPoolExecutor(max_workers=2) as executor:
        pending = [
            executor.submit(run, ["docker", "pull", image.reference], check=False)
            for image in remote_images
        ]
        return [
            image
            for image, future in zip(remote_images, pending)
            if future.result().returncode
        ]


def get_image_pair(
    base: str, architecture: str, *, dev: bool = False
) -> tuple[str, str, str]:
    if dev:
        return _get_cached_image_pair(base, dev=True)
    try:
        pair = _get_remote_image_pair(base, architecture)
    except _RegistryError:
        pair = None
        if _request_user_confirmation(
            "Registry inspection failed. Run az acr login --name qdkacr and retry?"
        ):
            run(["az", "acr", "login", "--name", "qdkacr"])
            try:
                pair = _get_remote_image_pair(base, architecture)
            except _RegistryError:
                pass
        if pair is None:
            try:
                cached = _get_cached_image_pair(base)
            except LauncherError:
                raise LauncherError(
                    "Cannot inspect the registry image pair; verify ACR access, Docker Buildx and connectivity."
                ) from None
            if _request_user_confirmation(
                "Cannot compare registry digests. Continue with the cached image pair?"
            ):
                return cached
            raise LauncherError("Use of the cached image pair was declined.")
    required = [
        image for image in pair if _get_local_image_metadata(image.image_id) is None
    ]
    if required:
        if not _request_user_confirmation(
            f"Pull required sandbox images for build {pair[0].build_id}?\n"
            + "\n".join(f"  {image.reference}" for image in required)
        ):
            try:
                return _get_cached_image_pair(base)
            except LauncherError:
                raise LauncherError(
                    "Downloads declined and no matching cached image pair is available."
                ) from None
        failed = _pull_images(required)
        if failed:
            if not _request_user_confirmation(
                "Image pulls failed. Run az acr login --name qdkacr and retry failed pulls?"
            ):
                raise LauncherError("Image pull failed.")
            run(["az", "acr", "login", "--name", "qdkacr"])
            if _pull_images(failed):
                raise LauncherError("Image pull failed after authentication retry.")
    for image in pair:
        local = _get_local_image_metadata(image.image_id)
        if (
            local is None
            or local.get("Id") != image.image_id
            or _get_build_id(local) != image.build_id
        ):
            raise LauncherError("Downloaded image does not match the selected release.")
    _, proxy_name = get_image_names_for_tag(pair[0].build_id)
    for name, image in zip((base, proxy_name), pair):
        tagged = _get_local_image_metadata(name)
        if tagged is None or tagged.get("Id") != image.image_id:
            run(["docker", "tag", image.image_id, name])
    return pair[0].image_id, pair[1].image_id, pair[0].build_id
