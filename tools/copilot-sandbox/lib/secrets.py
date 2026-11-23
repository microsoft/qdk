"""Instance PAT storage. Never return subprocess or HTTP error bodies."""

from __future__ import annotations

import ctypes
from datetime import datetime, timedelta, timezone
from email.utils import parsedate_to_datetime
import getpass
from pathlib import Path
import shlex
import sys
import urllib.error
import urllib.parse
import urllib.request
import warnings
import webbrowser

from .common import LauncherError, raise_if_link, register_secret, write_atomic, run
from .state import validate_sandbox_instance_name

SERVICE = "qdk-copilot-sandbox"


def _validate_token_text(token: str) -> str:
    token = token.strip()
    if (
        not token
        or any(character.isspace() for character in token)
        or not token.isascii()
        or "\0" in token
    ):
        raise LauncherError("The PAT must be a single nonempty ASCII value.")
    register_secret(token)
    return token


def _call_dpapi(data: bytes, *, decrypt: bool) -> bytes:
    if sys.platform != "win32":
        raise LauncherError("DPAPI is available only on Windows.")
    from ctypes import wintypes

    class Blob(ctypes.Structure):
        _fields_ = [("size", wintypes.DWORD), ("data", ctypes.POINTER(ctypes.c_ubyte))]

    buffer = ctypes.create_string_buffer(data)
    source = Blob(len(data), ctypes.cast(buffer, ctypes.POINTER(ctypes.c_ubyte)))
    destination = Blob()
    crypt = ctypes.WinDLL("crypt32", use_last_error=True)
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    function = crypt.CryptUnprotectData if decrypt else crypt.CryptProtectData
    function.argtypes = [
        ctypes.POINTER(Blob),
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_void_p,
        wintypes.DWORD,
        ctypes.POINTER(Blob),
    ]
    function.restype = wintypes.BOOL
    kernel.LocalFree.argtypes = [ctypes.c_void_p]
    kernel.LocalFree.restype = ctypes.c_void_p
    if not function(
        ctypes.byref(source), None, None, None, None, 1, ctypes.byref(destination)
    ):
        raise LauncherError("Windows DPAPI failed for this user.")
    try:
        return ctypes.string_at(destination.data, destination.size)
    finally:
        kernel.LocalFree(destination.data)


class TokenStore:
    def __init__(self, name: str, directory: Path):
        self.name = validate_sandbox_instance_name(name)
        self.directory = directory

    def load(self) -> str:
        setup = f"sandbox.py set-token --instance {self.name}"
        if sys.platform == "win32":
            path = raise_if_link(self.directory / "token.dpapi")
            try:
                return _validate_token_text(
                    _call_dpapi(path.read_bytes(), decrypt=True).decode("ascii")
                )
            except (OSError, UnicodeError):
                raise LauncherError(
                    f"Cannot load this instance's PAT; run '{setup}'."
                ) from None
        if sys.platform == "darwin":
            result = run(
                [
                    "security",
                    "find-generic-password",
                    "-s",
                    SERVICE,
                    "-a",
                    self.name,
                    "-w",
                ],
                check=False,
            )
        else:
            result = run(["pass", "show", f"{SERVICE}/{self.name}"], check=False)
        if result.returncode:
            raise LauncherError(
                f"Cannot load this instance's PAT; unlock the secret store or run '{setup}'."
            )
        return _validate_token_text(result.stdout)

    def save(self, token: str) -> None:
        token = _validate_token_text(token)
        if sys.platform == "win32":
            write_atomic(
                self.directory / "token.dpapi",
                _call_dpapi(token.encode("ascii"), decrypt=False),
            )
        elif sys.platform == "darwin":
            command = shlex.join(
                [
                    "add-generic-password",
                    "-U",
                    "-s",
                    SERVICE,
                    "-a",
                    self.name,
                    "-w",
                    token,
                ]
            )
            run(["security", "-i"], input_text=command + "\n")
        else:
            run(["gpg", "--version"])
            run(
                ["pass", "insert", "--multiline", "--force", f"{SERVICE}/{self.name}"],
                input_text=token + "\n",
            )

    def delete(self) -> None:
        if sys.platform == "win32":
            raise_if_link(self.directory / "token.dpapi").unlink(missing_ok=True)
        elif sys.platform == "darwin":
            result = run(
                ["security", "delete-generic-password", "-s", SERVICE, "-a", self.name],
                check=False,
            )
            if result.returncode not in (0, 44):
                raise LauncherError("Keychain deletion failed; the instance was kept.")
        else:
            result = run(
                ["pass", "rm", "--force", f"{SERVICE}/{self.name}"], check=False
            )
            if (
                result.returncode
                and "is not in the password store" not in result.stderr
            ):
                raise LauncherError(
                    "Password-store deletion failed; the instance was kept."
                )


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(
        self, req: object, fp: object, code: int, msg: str, headers: object, newurl: str
    ) -> None:
        return None


def validate_token_properties(
    token: str, *, instance: str = "default", now: datetime | None = None
) -> None:
    setup = (
        f"sandbox.py set-token --instance {validate_sandbox_instance_name(instance)}"
    )
    request = urllib.request.Request(
        "https://api.github.com/user",
        headers={
            "Authorization": f"Bearer {_validate_token_text(token)}",
            "Accept": "application/vnd.github+json",
            "User-Agent": SERVICE,
        },
    )
    try:
        with urllib.request.build_opener(NoRedirect()).open(
            request, timeout=20
        ) as response:
            expiry = response.headers.get("GitHub-Authentication-Token-Expiration")
    except urllib.error.HTTPError as error:
        if error.code == 401:
            raise LauncherError(
                f"GitHub rejected the PAT (401); run '{setup}'."
            ) from None
        raise LauncherError(
            f"GitHub PAT validation failed (HTTP status: {error.code}); check GitHub availability and PAT permissions."
        ) from None
    except (OSError, ValueError) as error:
        raise LauncherError(
            "GitHub PAT validation failed; check connectivity."
        ) from error
    if expiry:
        try:
            try:
                expiration = parsedate_to_datetime(expiry)
            except ValueError:
                expiration = datetime.fromisoformat(
                    expiry.replace("Z", "+00:00").replace(" UTC", "+00:00")
                )
            if expiration.tzinfo is None:
                expiration = expiration.replace(tzinfo=timezone.utc)
            if expiration <= (now or datetime.now(timezone.utc)) + timedelta(days=2):
                print(
                    f"Warning: this PAT expires within two days; rotate with '{setup}'."
                )
        except (ValueError, TypeError, OverflowError):
            print("Warning: GitHub returned an unreadable PAT expiration date.")


def request_and_store_token(store: TokenStore) -> None:
    query = urllib.parse.urlencode(
        {
            "name": f"QDK sandbox {store.name}",
            "expires_in": "7",
            "user_copilot_requests": "read",
        }
    )
    url = "https://github.com/settings/personal-access-tokens/new?" + query
    if (
        input("Do you want to open a browser to generate a new PAT? [y/N]: ")
        .strip()
        .lower()
        == "y"
    ):
        print("Launching a browser so you can create a fine-grained PAT on GitHub.")
        print(
            "Create a fine-grained PAT: public repositories only, Copilot Requests: read, seven-day expiry."
        )
        print(url)
        webbrowser.open(url)
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("error", getpass.GetPassWarning)
            token = _validate_token_text(getpass.getpass("Fine-grained PAT (hidden): "))
    except getpass.GetPassWarning:
        raise LauncherError(
            f"A terminal supporting hidden input is required for 'sandbox.py set-token --instance {store.name}'."
        ) from None
    validate_token_properties(token, instance=store.name)
    store.save(token)
    print("PAT stored for this instance.")
