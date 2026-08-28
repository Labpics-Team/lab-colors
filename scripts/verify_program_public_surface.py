#!/usr/bin/env python3
"""Fail closed when staged Program code reaches the rendered public Rust API."""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit


class RustdocShapeError(RuntimeError):
    """The rustdoc tree is incomplete or no longer has the verified shape."""


@dataclass(frozen=True)
class ProgramLeak:
    public_item: str
    route: str


class _AllItemsParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.depth = 0
        self.all_items_depth: int | None = None
        self.hrefs: list[str] = []

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        self.depth += 1
        values = dict(attrs)
        classes = set((values.get("class") or "").split())
        if tag == "ul" and "all-items" in classes:
            if self.all_items_depth is not None:
                raise RustdocShapeError("nested rustdoc all-items lists are ambiguous")
            self.all_items_depth = self.depth
        elif self.all_items_depth is not None and tag == "a":
            href = values.get("href")
            if href:
                self.hrefs.append(href)

    def handle_endtag(self, tag: str) -> None:
        if tag == "ul" and self.all_items_depth == self.depth:
            self.all_items_depth = None
        self.depth -= 1
        if self.depth < 0:
            raise RustdocShapeError("malformed rustdoc HTML nesting")


class _LinkParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.links: list[tuple[frozenset[str], str]] = []

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        if tag != "a":
            return
        values = dict(attrs)
        href = values.get("href")
        if href:
            self.links.append(
                (frozenset((values.get("class") or "").split()), href)
            )


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8-sig")
    except (OSError, UnicodeError) as error:
        raise RustdocShapeError(f"cannot read {path}: {error}") from error


def _inside(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def _resolve_local_link(page: Path, href: str, docs_root: Path) -> Path | None:
    parsed = urlsplit(href)
    if parsed.scheme or parsed.netloc or not parsed.path:
        return None
    decoded = unquote(parsed.path)
    if decoded.startswith("/"):
        raise RustdocShapeError(f"absolute local rustdoc link is unsupported: {href}")
    resolved = (page.parent / decoded).resolve()
    if not _inside(resolved, docs_root):
        raise RustdocShapeError(f"rustdoc link escapes its output tree: {href}")
    return resolved


def public_item_pages(crate_doc_root: Path) -> list[tuple[str, Path]]:
    crate_doc_root = crate_doc_root.resolve()
    docs_root = crate_doc_root.parent
    all_items = crate_doc_root / "all.html"
    parser = _AllItemsParser()
    parser.feed(_read(all_items))
    parser.close()
    if parser.all_items_depth is not None:
        raise RustdocShapeError("rustdoc all-items list is not closed")
    if not parser.hrefs:
        raise RustdocShapeError("rustdoc all.html contains no public item links")

    pages: list[tuple[str, Path]] = []
    seen: set[Path] = set()
    for href in parser.hrefs:
        page = _resolve_local_link(all_items, href, docs_root)
        if page is None or not _inside(page, crate_doc_root):
            raise RustdocShapeError(f"public item is not a local crate page: {href}")
        if page.suffix != ".html":
            raise RustdocShapeError(f"public item does not resolve to HTML: {href}")
        if page in seen:
            raise RustdocShapeError(f"duplicate public rustdoc item page: {href}")
        if not page.is_file():
            raise RustdocShapeError(f"public rustdoc item page is missing: {href}")
        seen.add(page)
        pages.append((href, page))
    return pages


def program_public_surface(crate_doc_root: Path) -> tuple[int, list[ProgramLeak]]:
    crate_doc_root = crate_doc_root.resolve()
    docs_root = crate_doc_root.parent
    forbidden_sources = tuple(
        (docs_root / f"src/labcolors_core/{source}.rs.html").resolve()
        for source in ("observation", "program", "program_session", "session")
    )
    forbidden_source_dirs = (
        (docs_root / "src/labcolors_core/program").resolve(),
    )
    forbidden_modules = tuple(
        (crate_doc_root / module).resolve()
        for module in ("observation", "program", "program_session", "session")
    )
    pages = public_item_pages(crate_doc_root)
    leaks: list[ProgramLeak] = []

    for public_item, page in pages:
        if any(_inside(page, module) for module in forbidden_modules):
            leaks.append(ProgramLeak(public_item, str(page.relative_to(docs_root))))
            continue

        parser = _LinkParser()
        parser.feed(_read(page))
        parser.close()
        source_links = 0
        for classes, href in parser.links:
            route = _resolve_local_link(page, href, docs_root)
            if route is None:
                continue
            if "src" in classes:
                source_links += 1
            if (
                route in forbidden_sources
                or any(_inside(route, source_dir) for source_dir in forbidden_source_dirs)
                or any(_inside(route, module) for module in forbidden_modules)
            ):
                leaks.append(ProgramLeak(public_item, href))
                break
        if source_links == 0:
            raise RustdocShapeError(
                "public rustdoc item has no compiler-emitted source link with "
                'expected rustdoc class "src"; rustdoc HTML markup or toolchain '
                f"version is incompatible: {public_item}"
            )

    return len(pages), leaks


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "crate_doc_root",
        nargs="?",
        default="target/doc/labcolors_core",
        type=Path,
    )
    args = parser.parse_args(argv)
    try:
        item_count, leaks = program_public_surface(args.crate_doc_root)
    except RustdocShapeError as error:
        print(f"Program public rustdoc surface: FAIL: {error}", file=sys.stderr)
        return 1
    if leaks:
        print("Program public rustdoc surface: FAIL:", file=sys.stderr)
        for leak in leaks:
            print(
                f"  public item {leak.public_item} reaches staged program via {leak.route}",
                file=sys.stderr,
            )
        return 1
    print(f"Program public rustdoc surface: PASS; public_items={item_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
