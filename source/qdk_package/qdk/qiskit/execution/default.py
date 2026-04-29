# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


from concurrent.futures import ThreadPoolExecutor


class DetaultExecutor(ThreadPoolExecutor):
    def __init__(self) -> None:
        super().__init__(max_workers=1)
