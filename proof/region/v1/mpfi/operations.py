#!/usr/bin/env python3
"""Machine-readable operation boundary for the MPFI comparator.

This file is intentionally small: it is the source-level gate for the
evaluator, not a claim about every symbol shipped by the MPFI distribution.
The dependency's broader API remains outside the comparator's authority.

The gate is deliberately conservative.  A forbidden dependency name is
rejected wherever it appears, not only when it is followed by ``(``: a macro,
function pointer, or assembler alias can otherwise hide the call from a
call-shaped regular expression.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


ENGINE = "mpfi"
COMPILER_FAMILY = "clang"
TARGET = "x86_64-pc-linux-gnu"
COMPILE_FLAGS = (
    "-std=c17",
    "-O2",
    "-fno-fast-math",
    "-ffp-contract=off",
    "-fno-lto",
    "-march=x86-64",
    "-mtune=generic",
)

# These are the only MPFI calls the evaluator is allowed to make.  GMP/MPFR
# calls used to construct exact inputs and to serialize witness bounds are a
# separate dependency boundary and are listed below rather than silently
# widened by a future source edit.
ALLOWED_MPFI_CALLS = frozenset(
    {
        "mpfi_abs",
        "mpfi_add",
        "mpfi_cbrt",
        "mpfi_clear",
        "mpfi_cos",
        "mpfi_div",
        "mpfi_exp",
        "mpfi_get_left",
        "mpfi_get_prec",
        "mpfi_get_right",
        "mpfi_has_zero",
        "mpfi_init2",
        "mpfi_intersect",
        "mpfi_interv_fr",
        "mpfi_interv_si",
        "mpfi_is_empty",
        "mpfi_is_nonneg",
        "mpfi_is_nonpos",
        "mpfi_is_pos",
        "mpfi_is_strictly_neg",
        "mpfi_is_strictly_pos",
        "mpfi_is_zero",
        "mpfi_log",
        "mpfi_mul",
        "mpfi_set",
        "mpfi_set_q",
        "mpfi_set_si",
        "mpfi_set_ui",
        "mpfi_sin",
        "mpfi_sqrt",
        "mpfi_sub",
        "mpfi_union",
    }
)

FORBIDDEN_MPFI_CALLS = frozenset(
    {
        "mpfi_atan2",
        "mpfi_div_ext",
        "mpfi_exp10",
        "mpfi_rec_sqrt",
        "mpfi_set_ld",
    }
)

ALLOWED_MPFR_CALLS = frozenset(
    {
        "mpfr_clear",
        "mpfr_clears",
        "mpfr_cmp",
        "mpfr_free_str",
        "mpfr_get_str",
        "mpfr_init2",
        "mpfr_inits2",
        "mpfr_max",
        "mpfr_min",
    }
)

ALLOWED_GMP_CALLS = frozenset(
    {
        "mpq_canonicalize",
        "mpq_clear",
        "mpq_cmp",
        "mpq_cmp_ui",
        "mpq_init",
        "mpq_mul",
        "mpq_set",
        "mpq_set_den",
        "mpq_set_num",
        "mpq_sgn",
        "mpq_sub",
        "mpz_clear",
        "mpz_init_set_ui",
        "mpz_mul_2exp",
        "mpz_neg",
    }
)

_CALL = re.compile(r"\b((?:mpfi|mpfr|mpq|mpz)_[A-Za-z0-9_]+)\s*\(")
def called_symbols(source: str) -> frozenset[str]:
    return frozenset(_CALL.findall(source))


def undefined_symbols(nm_output: str) -> frozenset[str]:
    """Extract dependency symbols from ``nm -u`` without trusting formatting."""

    symbols: set[str] = set()
    for line in nm_output.splitlines():
        fields = line.split()
        if not fields:
            continue
        symbol = fields[-1].lstrip("_")
        if symbol.startswith(("mpfi_", "mpfr_", "mpq_", "mpz_")):
            symbols.add(symbol)
    return frozenset(symbols)


def validate_undefined_symbols(nm_output: str) -> tuple[str, ...]:
    """Check the symbols the compiler left unresolved in evaluator objects."""

    seen = undefined_symbols(nm_output)
    allowed = ALLOWED_MPFI_CALLS | ALLOWED_MPFR_CALLS | ALLOWED_GMP_CALLS
    errors: list[str] = []
    errors.extend(
        f"forbidden undefined external symbol {symbol}"
        for symbol in sorted(seen & FORBIDDEN_MPFI_CALLS)
    )
    errors.extend(
        f"unexpected undefined external symbol {symbol}"
        for symbol in sorted(seen - allowed - FORBIDDEN_MPFI_CALLS)
    )
    return tuple(errors)


def validate_sources(directory: Path) -> tuple[str, ...]:
    paths = sorted(directory.glob("*.c")) + sorted(directory.glob("*.h"))
    if not paths:
        return ("evaluator source directory is empty",)
    errors: list[str] = []
    seen: set[str] = set()
    for path in paths:
        text = path.read_text(encoding="utf-8")
        seen.update(called_symbols(text))
        for forbidden in FORBIDDEN_MPFI_CALLS:
            if re.search(rf"\b{re.escape(forbidden)}\b", text):
                errors.append(f"{path.name}: forbidden operation {forbidden}")
        for marker in ("arb", "flint", "long double", "strtod", "fallback"):
            if marker in text.lower():
                errors.append(f"{path.name}: forbidden dependency marker {marker}")
    allowed = ALLOWED_MPFI_CALLS | ALLOWED_MPFR_CALLS | ALLOWED_GMP_CALLS
    errors.extend(
        f"unexpected external call {symbol}"
        for symbol in sorted(seen - allowed)
    )
    errors.extend(
        f"required MPFI call is absent {symbol}"
        for symbol in sorted(ALLOWED_MPFI_CALLS - seen)
    )
    return tuple(errors)


def main(argv: list[str]) -> int:
    if len(argv) == 2:
        errors = validate_sources(Path(argv[1]))
    elif len(argv) == 3 and argv[1] == "--undefined-symbols":
        errors = validate_undefined_symbols(Path(argv[2]).read_text(encoding="utf-8"))
    else:
        print(
            "usage: operations.py EVALUATOR_DIRECTORY | "
            "--undefined-symbols NM_OUTPUT",
            file=sys.stderr,
        )
        return 2
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
