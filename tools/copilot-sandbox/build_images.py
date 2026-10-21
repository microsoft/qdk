#!/usr/bin/env python3
"""Build the daily sandbox images or equivalent local images (Python 3.10+)."""

from __future__ import annotations

import argparse
import ast
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from typing import IO
from urllib.parse import urlsplit
import uuid

from lib.images import LOCAL_IMAGE_PREFIX, LOCAL_IMAGE_TAG

BASE = "mcr.microsoft.com/azurelinux/base/core:3.0"
SECRET_IDS = {"msrustup_access_token", "npmrc", "pip_index_url", "cargo_token"}


class BuildError(Exception):
    pass


def run_command(
    arguments: list[str], *, capture: bool = False, stdout: IO[bytes] | None = None
) -> bytes:
    try:
        result = subprocess.run(
            arguments,
            check=False,
            stdout=(
                stdout if stdout is not None else subprocess.PIPE if capture else None
            ),
        )
    except OSError:
        raise BuildError(f"Cannot start {arguments[0]}.") from None
    if result.returncode:
        raise BuildError(
            f"{' '.join(arguments[:2])} failed (exit {result.returncode})."
        )
    return result.stdout if capture else b""


def validate_pinned_base(reference: str) -> str:
    if not re.fullmatch(re.escape(BASE) + r"@sha256:[0-9a-f]{64}", reference):
        raise BuildError(
            "Base must be Azure Linux core:3.0 pinned with @sha256:<digest>."
        )
    return reference


def get_pinned_base() -> str:
    descriptor = json.loads(
        run_command(
            [
                "docker",
                "buildx",
                "imagetools",
                "inspect",
                BASE,
                "--format",
                "{{json .Manifest}}",
            ],
            capture=True,
        )
    )
    return validate_pinned_base(BASE + "@" + descriptor["digest"])


# Extract the rust version from prereqs.py
def get_rust_channel(repo: Path, *, package_source: str = "azure") -> str:
    module = ast.parse((repo / "prereqs.py").read_text(encoding="utf-8"))
    for statement in module.body:
        if isinstance(statement, ast.Assign) and any(
            isinstance(target, ast.Name) and target.id == "rust_ver"
            for target in statement.targets
        ):
            version = ast.literal_eval(statement.value)
            if (
                isinstance(version, tuple)
                and len(version) == 3
                and all(type(part) is int and part >= 0 for part in version)
            ):
                if package_source == "public":
                    return ".".join(str(part) for part in version)
                return f"ms-prod-{version[0]}.{version[1]}"
    raise BuildError("Cannot derive Rust channel from prereqs.py rust_ver.")


def unarchive_safe(stream: IO[bytes], destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=stream, mode="r|*") as archive:
        for member in archive:
            path = PurePosixPath(member.name)
            if path.is_absolute() or ".." in path.parts or "\\" in member.name:
                raise BuildError("Unsafe archive path.")
            if (
                path.name in {".pypirc", ".netrc"}
                or (
                    ".cargo" in path.parts
                    and path.name in {"credentials", "credentials.toml"}
                )
                or path.name.startswith(".env")
                or path.parts[0] in {".git", "exchange"}
            ):
                raise BuildError(
                    "Authentication or generated files must not be tracked in the source context."
                )
            target = destination.joinpath(*path.parts)
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
            elif member.isfile():
                source = archive.extractfile(member)
                if source is None:
                    raise BuildError("Unreadable archive member.")
                target.parent.mkdir(parents=True, exist_ok=True)
                with source, target.open("wb") as output:
                    shutil.copyfileobj(source, output)
                target.chmod(member.mode & 0o777)
            else:
                raise BuildError("Archive links and special files are not accepted.")


def snapshot_repo(repo: Path, destination: Path) -> str:
    if run_command(
        ["git", "-C", str(repo), "status", "--porcelain"], capture=True
    ).strip():
        print(
            "Warning: only committed HEAD is exported; working-tree changes are excluded.",
            file=sys.stderr,
        )
    commit = (
        run_command(["git", "-C", str(repo), "rev-parse", "HEAD"], capture=True)
        .decode()
        .strip()
    )
    if not re.fullmatch(r"[0-9a-f]{40,64}", commit):
        raise BuildError("Invalid source commit.")
    with tempfile.TemporaryFile() as archive:
        run_command(
            ["git", "-C", str(repo), "archive", "--format=tar", commit], stdout=archive
        )
        archive.seek(0)
        unarchive_safe(archive, destination)
    return commit


def load_secrets(args: argparse.Namespace) -> list[str]:
    specifications = {}
    if args.installer:
        installer = args.installer.resolve(strict=True)
        if not installer.is_file() or "," in str(installer):
            raise BuildError(
                "Installer must be a regular file without commas in its path."
            )
        specifications["msrustup_installer"] = f"id=msrustup_installer,src={installer}"
    for kind, inputs in (("src", args.secret_file), ("env", args.secret_env)):
        for item in inputs:
            identifier, separator, location = item.partition("=")
            if (
                not separator
                or identifier not in SECRET_IDS
                or identifier in specifications
            ):
                raise BuildError("Invalid or duplicate BuildKit secret specification.")
            if kind == "src":
                path = Path(location).resolve(strict=True)
                if (
                    not path.is_file()
                    or "," in str(path)
                    or path.is_relative_to(args.repo.resolve())
                ):
                    raise BuildError(
                        "Secret files must be regular files outside the checkout."
                    )
                location = str(path)
            elif (
                not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", location)
                or not os.environ.get(location, "").strip()
            ):
                raise BuildError(
                    "A required secret environment variable is missing or empty."
                )
            specifications[identifier] = f"id={identifier},{kind}={location}"
    if "agent" in args.target:
        if args.package_source == "public":
            if specifications:
                raise BuildError("Build secrets require --package-source azure.")
            return []
        required = {
            "msrustup_installer",
            "msrustup_access_token",
            "npmrc",
            "pip_index_url",
        }
        if not required.issubset(specifications):
            raise BuildError(
                "Azure builds require installer, msrustup_access_token, npmrc and pip_index_url secrets."
            )
        if "cargo_token" not in specifications:
            specifications["cargo_token"] = (
                "id=cargo_token,"
                + specifications["msrustup_access_token"].partition(",")[2]
            )
    return list(specifications.values())


def stage_docker_build_context(source: Path, destination: Path) -> None:
    def excluded(directory: str, names: list[str]) -> set[str]:
        return {
            name
            for name in names
            if name
            in {".npmrc", ".netrc", ".pypirc", ".git", "node_modules", "__pycache__"}
            or name.startswith(".env")
        }

    shutil.copytree(source, destination, ignore=excluded, symlinks=True)
    if any(path.is_symlink() for path in destination.rglob("*")):
        raise BuildError("Docker context symlinks are not accepted.")


def normalize_feed_uri(feed: str) -> str:
    feed = feed.removeprefix("sparse+")
    parsed = urlsplit(feed)
    if feed and (
        parsed.scheme != "https"
        or not (
            parsed.hostname == "pkgs.dev.azure.com"
            or (parsed.hostname or "").endswith(".pkgs.visualstudio.com")
        )
        or parsed.username
        or parsed.password
        or parsed.query
        or parsed.fragment
        or any(character.isspace() for character in feed)
        or "$(" in feed
    ):
        raise BuildError(
            "Feed arguments must be credential-free Azure Artifacts HTTPS URLs."
        )
    return feed


def get_repo_build_arguments(args: argparse.Namespace) -> list[str]:
    arguments = args.build_args
    if arguments[:1] == ["--"]:
        return arguments[1:]
    return arguments or ["--qdk", "--no-check", "--no-test"]


def build_images(args: argparse.Namespace) -> None:
    args.target = list(dict.fromkeys(args.target or ["agent", "proxy"]))
    args.toolchain_feed = normalize_feed_uri(args.toolchain_feed)
    args.crates_feed = normalize_feed_uri(args.crates_feed)
    if "agent" in args.target:
        if args.package_source == "azure" and not (
            args.toolchain_feed and args.crates_feed
        ):
            raise BuildError(
                "Azure builds require explicit toolchain and crates feed URLs."
            )
        if args.package_source == "public" and (
            args.toolchain_feed or args.crates_feed
        ):
            raise BuildError("Feed arguments require --package-source azure.")
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,100}", args.tag) or (
        args.registry
        and not re.fullmatch(r"[a-z0-9][a-z0-9.:-]*(?:/[a-z0-9._-]+)*", args.registry)
    ):
        raise BuildError("Invalid image tag or registry prefix.")
    architecture = {
        "x86_64": "amd64",
        "aarch64": "arm64",
        "amd64": "amd64",
        "arm64": "arm64",
    }.get(platform.machine().lower())
    if architecture is None or (
        args.platform and args.platform != "linux/" + architecture
    ):
        raise BuildError("Build on a native linux/amd64 or linux/arm64 worker.")
    specifications = load_secrets(args)
    base = (
        validate_pinned_base(args.base_image) if args.base_image else get_pinned_base()
    )
    repo = args.repo.resolve(strict=True)
    build_id = args.tag if args.registry else "local-" + uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix="qdk-sandbox-") as temporary:
        root = Path(temporary)
        source = root / "qdk"
        commit = snapshot_repo(repo, source)
        for target in args.target:
            context = root / target
            stage_docker_build_context(
                repo / ".ado/images/copilot-sandbox" / target, context
            )
            if re.search(
                r"^\s*#\s*syntax\s*=",
                (context / "Dockerfile").read_text(),
                re.MULTILINE | re.IGNORECASE,
            ):
                raise BuildError("Dockerfile frontend downloads are forbidden.")
            image = (
                f"{args.registry}/copilot-sandbox/{target}:{args.tag}-{architecture}"
                if args.registry
                else f"{LOCAL_IMAGE_PREFIX}{target}:{args.tag}"
            )
            arguments = [
                "docker",
                "build",
                "--pull",
                "--no-cache",
                "--platform",
                "linux/" + architecture,
                "--tag",
                image,
                "--build-arg",
                "BASE_IMAGE=" + base,
                "--build-arg",
                "BUILD_ID=" + build_id,
                "--build-arg",
                "QDK_SOURCE_SHA=" + commit,
            ]
            if target == "agent":
                arguments.extend(
                    [
                        "--build-context",
                        "qdk=" + str(source),
                        "--build-arg",
                        "PACKAGE_SOURCE=" + args.package_source,
                        "--build-arg",
                        "RUST_CHANNEL="
                        + get_rust_channel(source, package_source=args.package_source),
                        "--build-arg",
                        "TOOLCHAIN_FEED=" + args.toolchain_feed,
                        "--build-arg",
                        "CRATES_FEED=" + args.crates_feed,
                        "--build-arg",
                        "QDK_BUILD_ARGS=" + json.dumps(get_repo_build_arguments(args)),
                    ]
                )
                for specification in specifications:
                    arguments.extend(["--secret", specification])
            run_command([*arguments, str(context)])
            if not args.registry:
                identity = (
                    run_command(
                        ["docker", "image", "inspect", "--format", "{{.Id}}", image],
                        capture=True,
                    )
                    .decode()
                    .strip()
                )
                if not re.fullmatch(r"sha256:[0-9a-f]{64}", identity):
                    raise BuildError(
                        "Cannot identify the built image for local pairing."
                    )
                run_command(
                    [
                        "docker",
                        "tag",
                        identity,
                        f"{LOCAL_IMAGE_PREFIX}{target}:{build_id}",
                    ]
                )


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    actions.add_parser(
        "resolve-base", help="Resolve the approved MCR base to an index digest."
    )
    builder = actions.add_parser(
        "build", help="Build and verify images; never pushes or logs in."
    )
    builder.add_argument(
        "--repo", type=Path, default=Path(__file__).resolve().parents[2]
    )
    builder.add_argument("--installer", type=Path)
    builder.add_argument(
        "--package-source",
        choices=("public", "azure"),
        default="public",
        help="Use public package sources (default) or authenticated Azure Artifacts feeds.",
    )
    builder.add_argument(
        "--secret-file", action="append", default=[], metavar="ID=PATH"
    )
    builder.add_argument(
        "--secret-env", action="append", default=[], metavar="ID=VARIABLE"
    )
    builder.add_argument("--toolchain-feed", default="")
    builder.add_argument("--crates-feed", "--feed", dest="crates_feed", default="")
    builder.add_argument(
        "--base-image", help="Approved digest-pinned base; otherwise resolved from MCR."
    )
    builder.add_argument("--platform", choices=("linux/amd64", "linux/arm64"))
    builder.add_argument(
        "--target", choices=("agent", "proxy"), action="append", default=[]
    )
    builder.add_argument(
        "--registry",
        help="Registry prefix for pipeline tags; omitted for local images.",
    )
    builder.add_argument("--tag", default=LOCAL_IMAGE_TAG)
    builder.add_argument(
        "build_args",
        nargs=argparse.REMAINDER,
        help="build.py arguments after -- (default: --qdk --no-check --no-test)",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    try:
        args = parse_args(argv)
        if args.action == "resolve-base":
            print(get_pinned_base())
        else:
            build_images(args)
    except (
        BuildError,
        OSError,
        ValueError,
        KeyError,
        SyntaxError,
        tarfile.TarError,
    ) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
