"""Agent-written volumes, which the host reads only through copies made by helper containers."""

from __future__ import annotations

import json
from pathlib import Path

from .common import LauncherError, read_json, run, sanitize_diagnostic, write_json
from .layout import AGENT_USER, Mount, helper_arguments, volume_name
from .podman import podman, vm_path

# Runs as container root. The agent can't push to an empty volume, so make the outbox a bare repository.
# Only an empty volume: running Git on anything the agent wrote would use its configuration.
_PREPARE = f"""
set -eu
if [ -z "$(ls -A /volumes/outbox)" ]; then
    git -c init.templateDir= init --quiet --bare /volumes/outbox
    chown -R -P {AGENT_USER} /volumes/outbox
fi
chown {AGENT_USER} /volumes/outbox /volumes/output
"""

# Runs as container root, which is you on the host, so every copy is yours. Arguments: the outbox
# and output volumes, then the host's outbox copy and output folder. Reads the previous state (output
# manifest and problems already reported) as JSON on stdin and prints JSON: the outbox's branch refs,
# read as data so no host file system's casing rules apply to them, the new state, and new problems.
# It copies only directories and regular files: links, special files, output Git metadata and names
# that would misbehave on macOS or Windows never reach the host. The manifest records what each output
# copy wrote, so files you change afterwards are left alone, and files you delete come back only if
# the agent updates them.
COPY_OUT_SCRIPT = r"""
import collections
import json
import os
import re
import shutil
import stat
import sys
import tempfile
import unicodedata

source_outbox, source_output, target_outbox, target_output = sys.argv[1:5]
state = json.load(sys.stdin)
previous = state.get("files", {})
reported = set(state.get("reported", []))
manifest = {}
problems = []
# Names Windows can't open or delete normally, and names Git treats as .git on NTFS.
UNSAFE_NAME = re.compile(
    r"(?i)(?:con|prn|aux|nul|com[0-9\u00b9\u00b2\u00b3]|lpt[0-9\u00b9\u00b2\u00b3])(?:\..*)?|git~\d+|.*[. ]"
    r"|.*[<>:\"|?*\\\x00-\x1f].*"
)
# Lowercase hexadecimal names, so no file system's case folding can merge two of them.
OBJECT = re.compile(
    r"[0-9a-f]{2}/[0-9a-f]{38}(?:[0-9a-f]{24})?"
    r"|pack/pack-[0-9a-f]{40}(?:[0-9a-f]{24})?\.(?:pack|idx|rev|bitmap|mtimes)"
)


def note(path, reason):
    problems.append(f"{ascii(path)}: {reason}")


def fail(message):
    print(message, file=sys.stderr)
    sys.exit(2)


def require_real(path):
    # The agent can replace any directory it owns with a link; never traverse one.
    if os.path.lexists(path) and os.path.realpath(path) != os.path.abspath(path):
        fail(f"{ascii(path)} is or passes through a link.")


def signature(info):
    return [info.st_size, info.st_mtime_ns]


def write_file(source, target, info):
    # A fresh, uniquely named file, so nothing already in the folder is ever removed by mistake.
    descriptor, temporary = tempfile.mkstemp(prefix=".sandbox-copy-", dir=os.path.dirname(target))
    try:
        nofollow = lambda path, flags: os.open(path, flags | os.O_NOFOLLOW)
        with os.fdopen(descriptor, "wb") as writer, open(source, "rb", opener=nofollow) as reader:
            shutil.copyfileobj(reader, writer, 1 << 20)
        os.chmod(temporary, 0o644)
        os.utime(temporary, ns=(info.st_atime_ns, info.st_mtime_ns))
        os.replace(temporary, target)
    except BaseException:
        os.unlink(temporary)
        raise
    return os.stat(target)


# For files the host keeps no history of, such as outbox objects.
def copy_file(source, target, info):
    if os.path.lexists(target):
        if os.path.islink(target) or not os.path.isfile(target):
            note(target, "exists and isn't a regular file")
            return
        if signature(os.stat(target)) == signature(info):
            return
    write_file(source, target, info)


def same_content(first, second):
    with open(first, "rb") as left, open(second, "rb") as right:
        while True:
            chunk = left.read(1 << 20)
            if chunk != right.read(1 << 20):
                return False
            if not chunk:
                return True


def copy_output_file(source, target, relative, info):
    record = previous.get(relative)
    wanted = signature(info)
    if os.path.lexists(target):
        if os.path.islink(target) or not os.path.isfile(target):
            note(target, "exists and isn't a regular file")
            return
        current = signature(os.stat(target))
        if record and record["host"] == current:
            if record["source"] == wanted:
                manifest[relative] = record
                return
            # Unchanged since the last copy, so take the agent's update.
        elif os.path.getsize(target) == info.st_size and same_content(source, target):
            manifest[relative] = {"source": wanted, "host": current}
            return
        else:
            conflict = [current, wanted]
            if not record or record.get("conflict") != conflict:
                note(target, "kept; it changed on the host or wasn't copied by the sandbox")
            manifest[relative] = {**(record or {"source": None, "host": None}), "conflict": conflict}
            return
    elif record and record["source"] == wanted:
        # Deleted on the host since the last copy; don't bring it back.
        manifest[relative] = record
        return
    os.makedirs(os.path.dirname(target), mode=0o755, exist_ok=True)
    manifest[relative] = {"source": wanted, "host": signature(write_file(source, target, info))}


def mirror_objects(source, target):
    require_real(source)
    wanted = {}
    for parent, _, files in os.walk(source):
        for name in files:
            path = os.path.join(parent, name)
            relative = os.path.relpath(path, source).replace(os.sep, "/")
            info = os.lstat(path)
            if OBJECT.fullmatch(relative) and stat.S_ISREG(info.st_mode):
                wanted[relative] = info
    for relative, info in wanted.items():
        destination = os.path.join(target, relative)
        try:
            os.makedirs(os.path.dirname(destination), mode=0o755, exist_ok=True)
            copy_file(os.path.join(source, relative), destination, info)
        except OSError as error:
            note(destination, error.strerror or type(error).__name__)
    for parent, directories, files in os.walk(target, topdown=False):
        for name in files:
            path = os.path.join(parent, name)
            if os.path.relpath(path, target).replace(os.sep, "/") not in wanted:
                os.unlink(path)
        for name in directories:
            path = os.path.join(parent, name)
            if not os.path.islink(path) and not os.listdir(path):
                os.rmdir(path)


def read_refs(outbox):
    refs = {}
    packed = os.path.join(outbox, "packed-refs")
    require_real(packed)
    if os.path.isfile(packed):
        if os.path.getsize(packed) > 16 * 1024 * 1024:
            fail("Outbox packed refs exceed the safety limit.")
        with open(packed, encoding="ascii") as stream:
            for line in stream.read().splitlines():
                if line and not line.startswith(("#", "^")):
                    oid, separator, name = line.partition(" ")
                    if not separator:
                        fail("Malformed outbox packed refs.")
                    if name.startswith("refs/heads/"):
                        refs[name] = oid
    heads = os.path.join(outbox, "refs", "heads")
    require_real(os.path.join(outbox, "refs"))
    require_real(heads)
    for parent, directories, files in os.walk(heads):
        for name in (*directories, *files):
            path = os.path.join(parent, name)
            info = os.lstat(path)
            if stat.S_ISLNK(info.st_mode) or not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode)):
                note(path, "skipped link or special file")
            # A paused agent may be mid ref update; Git ignores lock files too.
            elif stat.S_ISREG(info.st_mode) and not name.endswith(".lock"):
                if info.st_size > 256:
                    fail("Malformed outbox branch ref.")
                with open(path, "rb") as stream:
                    value = stream.read().decode("ascii").strip()
                refs["refs/heads/" + os.path.relpath(path, heads).replace(os.sep, "/")] = value
    return refs


def copy_tree(source, target, prefix=""):
    if os.path.lexists(target) and (os.path.islink(target) or not os.path.isdir(target)):
        note(target, "exists and isn't a directory")
        return
    entries = sorted(os.scandir(source), key=lambda entry: entry.name)
    # Names macOS or Windows would treat as one file: differing in case or Unicode normalization.
    key = lambda name: unicodedata.normalize("NFC", name).casefold()
    folded = collections.Counter(key(entry.name) for entry in entries)
    for entry in entries:
        path = os.path.join(target, entry.name)
        relative = prefix + entry.name
        try:
            if key(entry.name) == ".git":
                note(entry.path, "skipped Git metadata")
            elif UNSAFE_NAME.fullmatch(entry.name):
                note(entry.path, "skipped; macOS or Windows can't handle the name safely")
            elif folded[key(entry.name)] > 1:
                note(entry.path, "skipped; another name differs only in case or Unicode form")
            elif entry.is_dir(follow_symlinks=False):
                copy_tree(entry.path, path, relative + "/")
            elif entry.is_file(follow_symlinks=False):
                copy_output_file(entry.path, path, relative, entry.stat(follow_symlinks=False))
            else:
                note(entry.path, "skipped link or special file")
        except (OSError, RecursionError) as error:
            note(entry.path, getattr(error, "strerror", None) or type(error).__name__)


# Earlier copies also held refs as files; the host now gets refs only as data.
for stale in ("refs", "packed-refs"):
    path = os.path.join(target_outbox, stale)
    if os.path.isdir(path) and not os.path.islink(path):
        shutil.rmtree(path)
    elif os.path.lexists(path):
        os.unlink(path)
mirror_objects(os.path.join(source_outbox, "objects"), os.path.join(target_outbox, "objects"))
refs = read_refs(source_outbox)
if len(refs) > 10000:
    fail("The outbox has more than 10000 branches.")
if target_output:
    # Output trouble, such as a very deep tree, mustn't stop the commits from being fetched.
    try:
        copy_tree(source_output, target_output)
    except (OSError, RecursionError, ValueError) as error:
        note(source_output, "copy stopped: " + type(error).__name__)
    # Keep records of files the agent removed, so a later copy doesn't resurrect your deletions either.
    manifest = {**{key: value for key, value in previous.items() if key not in manifest}, **manifest}
print(
    json.dumps(
        {
            "refs": refs,
            "state": {"files": manifest, "reported": sorted(set(problems))},
            # Each problem once, rather than on every fetch while it persists.
            "problems": [problem for problem in problems if problem not in reported],
        }
    )
)
"""


def prepare_agent_volumes(image: str, instance: str) -> None:
    mounts = [
        Mount("volume", volume_name(instance, kind), f"/volumes/{kind}", False)
        for kind in ("outbox", "output")
    ]
    run(podman(*helper_arguments(image, instance, mounts, "sh", "-c", _PREPARE)))


def copy_out_agent_volumes(
    image: str, instance: str, outbox: Path, outgoing: Path, manifest_path: Path
) -> dict[str, str]:
    """Copies the outbox's objects to outbox and new output to outgoing.

    Returns the outbox's branch refs, which the caller must validate."""
    mounts = [
        Mount("volume", volume_name(instance, "outbox"), "/source/outbox", True),
        Mount("volume", volume_name(instance, "output"), "/source/output", True),
        Mount("bind", vm_path(outbox), "/target/outbox", False),
        Mount("bind", vm_path(outgoing), "/target/output", False),
    ]
    previous = read_json(manifest_path).get("state")
    result = run(
        podman(
            *helper_arguments(
                image,
                instance,
                mounts,
                "/usr/bin/python3",
                "-I",
                "-c",
                COPY_OUT_SCRIPT,
                "/source/outbox",
                "/source/output",
                "/target/outbox",
                "/target/output",
                interactive=True,
            )
        ),
        input_text=json.dumps(previous if isinstance(previous, dict) else {}),
        check=False,
    )
    if result.returncode:
        raise LauncherError(
            f"Copying out the agent's volumes failed (exit {result.returncode}).\n"
            + sanitize_diagnostic(result.stderr)
        )
    try:
        report = json.loads(result.stdout)
        refs = report["refs"]
        problems = report["problems"]
        if not isinstance(refs, dict) or not all(
            isinstance(key, str) and isinstance(value, str)
            for key, value in refs.items()
        ):
            raise ValueError("Unexpected refs.")
        if not isinstance(report["state"], dict):
            raise ValueError("Unexpected state.")
    except (ValueError, KeyError, TypeError) as error:
        raise LauncherError("Cannot read the copy-out report.") from error
    write_json(manifest_path, {"state": report["state"]})
    for problem in problems:
        print("Skipped while copying: " + sanitize_diagnostic(str(problem)))
    return refs


def volume_exists(instance: str) -> bool:
    return (
        run(
            podman("volume", "exists", volume_name(instance, "outbox")), check=False
        ).returncode
        == 0
    )
