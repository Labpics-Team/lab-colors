#!/usr/bin/env python3
"""Cargo-mutants adapter: bind the actual isolated source before native Cargo."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
from mutation import ContractError, run_mutation_cargo

try:
    run_mutation_cargo(Path(__file__).resolve().parents[1], sys.argv[1:])
except (ContractError, OSError, ValueError) as error:
    # Preparation failure is evidence failure, never a semantically killed mutant.
    print(f"mutation cargo preparation error: {error}", file=sys.stderr)
    raise SystemExit(2)
