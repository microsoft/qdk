from collections.abc import Hashable
from typing import Collection
import multiprocessing
from concurrent.futures import ProcessPoolExecutor


def collection_is_persistent(collection: Collection[Hashable]) -> bool:
    with ProcessPoolExecutor(
        1, mp_context=multiprocessing.get_context("spawn")
    ) as executor:
        persisted = executor.submit(set, collection).result()
    local = set(collection)
    return local == persisted
