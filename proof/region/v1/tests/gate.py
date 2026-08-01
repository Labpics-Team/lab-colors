#!/usr/bin/env python3
"""Точный состав общего fast proof-suite без engine-specific зависимости."""

from __future__ import annotations

import hashlib
import unittest
from collections.abc import Iterator
from pathlib import Path


TEST_DIRECTORY = Path(__file__).resolve().parent
EXPECTED_TEST_COUNT_V1 = 166
EXPECTED_TEST_INVENTORY_SHA256_V1 = (
    "1690f11e76b57e532f9ca5fa7cccd298438b72fb9a5327192e57c3d84c899715"
)


def _iter_tests_v1(suite: unittest.TestSuite) -> Iterator[unittest.TestCase]:
    for item in suite:
        if isinstance(item, unittest.TestSuite):
            yield from _iter_tests_v1(item)
        elif isinstance(item, unittest.TestCase):
            yield item
        else:
            raise TypeError("suite contains a non-test object")


def _inventory_preimage_v1(test_ids: tuple[str, ...]) -> bytes:
    return b"".join(test_id.encode("utf-8") + b"\n" for test_id in sorted(test_ids))


def test_count_v1(suite: unittest.TestSuite) -> int:
    return sum(1 for _test in _iter_tests_v1(suite))


def test_inventory_sha256_v1(suite: unittest.TestSuite) -> str:
    test_ids = tuple(test.id() for test in _iter_tests_v1(suite))
    return hashlib.sha256(_inventory_preimage_v1(test_ids)).hexdigest()


def full_suite_v1() -> unittest.TestSuite:
    """Один engine-neutral suite, который CI уже запускает целиком."""

    return unittest.defaultTestLoader.discover(str(TEST_DIRECTORY), pattern="test_*.py")


def inventory_is_exact_v1(suite: unittest.TestSuite) -> bool:
    """Не даёт исчезнуть contract-тесту за общим minimum-count порогом CI."""

    return (
        test_count_v1(suite) == EXPECTED_TEST_COUNT_V1
        and test_inventory_sha256_v1(suite) == EXPECTED_TEST_INVENTORY_SHA256_V1
    )
