"""Check the formal negative controls against the PR tree before running Kani."""

import unittest
from pathlib import Path

from formal_contracts import MUTANTS


class FormalMutationSitesTests(unittest.TestCase):
    def test_each_mutation_changes_one_source_site(self):
        core = Path(__file__).resolve().parents[1] / "crates/labcolors-core/src"
        for name, path, _, _, original, replacement in MUTANTS:
            with self.subTest(mutant=name):
                self.assertNotEqual(original, replacement)
                self.assertEqual((core / path).read_bytes().count(original.encode()), 1)
