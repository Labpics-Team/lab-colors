#!/usr/bin/env python3
"""Mutation and fail-closed tests for the resolved rustdoc surface gate."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from verify_program_public_surface import (
    RustdocShapeError,
    program_public_surface,
)


class ProgramPublicSurfaceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.docs = Path(self.temporary.name) / "doc"
        self.crate = self.docs / "labcolors_core"
        self.crate.mkdir(parents=True)

    def write_all(self, *hrefs: str) -> None:
        links = "".join(f'<li><a href="{href}">item</a></li>' for href in hrefs)
        (self.crate / "all.html").write_text(
            f'<html><ul class="all-items">{links}</ul></html>',
            encoding="utf-8",
        )

    def write_item(self, relative: str, body: str) -> None:
        path = self.crate / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(f"<html>{body}</html>", encoding="utf-8")

    def test_clean_resolved_source_is_accepted(self) -> None:
        self.write_all("struct.Srgb8.html")
        self.write_item(
            "struct.Srgb8.html",
            '<a class="src" href="../src/labcolors_core/srgb8.rs.html#1">Source</a>',
        )
        count, leaks = program_public_surface(self.crate)
        self.assertEqual(count, 1)
        self.assertEqual(leaks, [])

    def test_arbitrary_root_reexport_alias_is_rejected_by_origin(self) -> None:
        self.write_all("struct.AnyClientName.html")
        self.write_item(
            "struct.AnyClientName.html",
            '<a class="src" href="../src/labcolors_core/program.rs.html#1031">Source</a>',
        )
        count, leaks = program_public_surface(self.crate)
        self.assertEqual(count, 1)
        self.assertEqual([leak.public_item for leak in leaks], ["struct.AnyClientName.html"])

    def test_arbitrary_session_alias_is_rejected_by_origin(self) -> None:
        self.write_all("struct.SessionAlias.html")
        self.write_item(
            "struct.SessionAlias.html",
            '<a class="src" href="../src/labcolors_core/session.rs.html#367">Source</a>',
        )
        _, leaks = program_public_surface(self.crate)
        self.assertEqual(len(leaks), 1)

    def test_arbitrary_program_session_alias_is_rejected_by_origin(self) -> None:
        self.write_all("struct.ProgramSessionAlias.html")
        self.write_item(
            "struct.ProgramSessionAlias.html",
            (
                '<a class="src" href="../src/labcolors_core/'
                'program_session.rs.html#2729">Source</a>'
            ),
        )
        _, leaks = program_public_surface(self.crate)
        self.assertEqual(len(leaks), 1)

    def test_arbitrary_observation_alias_is_rejected_by_origin(self) -> None:
        self.write_all("struct.ObservationAlias.html")
        self.write_item(
            "struct.ObservationAlias.html",
            (
                '<a class="src" href="../src/labcolors_core/'
                'observation.rs.html#249">Source</a>'
            ),
        )
        _, leaks = program_public_surface(self.crate)
        self.assertEqual(len(leaks), 1)

    def test_nested_alias_is_rejected_without_a_name_allowlist(self) -> None:
        self.write_all("facade/struct.UnrelatedName.html")
        self.write_item(
            "facade/struct.UnrelatedName.html",
            '<a class="src" href="../../src/labcolors_core/program.rs.html#1031">Source</a>',
        )
        _, leaks = program_public_surface(self.crate)
        self.assertEqual(len(leaks), 1)

    def test_nested_program_source_file_is_rejected_by_origin(self) -> None:
        self.write_all("struct.AttachmentAlias.html")
        self.write_item(
            "struct.AttachmentAlias.html",
            (
                '<a class="src" href="../src/labcolors_core/'
                'program/attachment.rs.html#1">Source</a>'
            ),
        )
        _, leaks = program_public_surface(self.crate)
        self.assertEqual(len(leaks), 1)

    def test_public_type_alias_route_to_program_is_rejected(self) -> None:
        self.write_all("type.Alias.html")
        self.write_item(
            "type.Alias.html",
            (
                '<a class="src" href="../src/labcolors_core/lib.rs.html#1">Source</a>'
                '<a href="program/struct.DraftV1.html">Draft</a>'
            ),
        )
        _, leaks = program_public_surface(self.crate)
        self.assertEqual(len(leaks), 1)

    def test_missing_item_page_fails_closed(self) -> None:
        self.write_all("struct.Missing.html")
        with self.assertRaises(RustdocShapeError):
            program_public_surface(self.crate)

    def test_missing_source_class_names_the_rustdoc_toolchain_contract(self) -> None:
        self.write_all("struct.Srgb8.html")
        self.write_item(
            "struct.Srgb8.html",
            '<a href="../src/labcolors_core/srgb8.rs.html#1">Source</a>',
        )
        with self.assertRaisesRegex(
            RustdocShapeError,
            'expected rustdoc class "src".*markup or toolchain version is incompatible',
        ):
            program_public_surface(self.crate)

    def test_empty_public_inventory_fails_closed(self) -> None:
        self.write_all()
        with self.assertRaises(RustdocShapeError):
            program_public_surface(self.crate)


if __name__ == "__main__":
    unittest.main(verbosity=2)
