import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from lib.boundary import _verify_device_requests
from lib.common import LauncherError


class GpuBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.request = {
            "Driver": "nvidia",
            "Count": -1,
            "DeviceIDs": None,
            "Capabilities": [["gpu"]],
            "Options": {},
        }

    def test_allows_no_device_requests(self):
        for requests in (None, []):
            with self.subTest(requests=requests):
                _verify_device_requests({"DeviceRequests": requests}, "agent")

    def test_accepts_expected_nvidia_request(self):
        _verify_device_requests({"DeviceRequests": [self.request]}, "agent")

    def test_proxy_cannot_request_gpus(self):
        with self.assertRaises(LauncherError):
            _verify_device_requests({"DeviceRequests": [self.request]}, "proxy")

    def test_rejects_multiple_requests(self):
        with self.assertRaises(LauncherError):
            _verify_device_requests(
                {"DeviceRequests": [self.request, self.request]}, "agent"
            )

    def test_rejects_different_driver_selection_or_capabilities(self):
        for field, value in (
            ("Driver", "other"),
            ("Count", 1),
            ("DeviceIDs", ["0"]),
            ("Capabilities", [["gpu", "video"]]),
            ("Capabilities", [["gpu"], ["other"]]),
            ("Options", {"other": "enabled"}),
        ):
            request = copy.deepcopy(self.request)
            request[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(LauncherError):
                _verify_device_requests(
                    {"DeviceRequests": [request]}, "agent"
                )


class GpuImageTests(unittest.TestCase):
    def test_headless_icd_uses_nvidia_egl(self):
        path = (
            Path(__file__).resolve().parents[3]
            / ".ado/images/sandbox-agent/runtime/nvidia-egl-icd.json"
        )
        manifest = json.loads(path.read_text())
        self.assertEqual(manifest["file_format_version"], "1.0.0")
        self.assertEqual(manifest["ICD"]["library_path"], "libEGL_nvidia.so.0")
        self.assertEqual(manifest["ICD"]["api_version"], "1.3.0")


if __name__ == "__main__":
    unittest.main()