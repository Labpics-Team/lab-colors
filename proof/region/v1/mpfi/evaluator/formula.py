#!/usr/bin/env python3
"""Generate the MPFI evaluator from the registered exact-real SSA.

This is a separate parser and emitter from the Arb implementation.  The two
engines intentionally consume the same immutable mathematical contract while
owning different C types, adapters and source identities.
"""

from __future__ import annotations

import hashlib
import sys
from dataclasses import dataclass
from pathlib import Path


SOURCE_SHA256 = "a6f77ac462f226453b1c27bbd8637b62780b9a640c317a6f50028dacd1de8540"
RELEASE_DOMAIN = b"labcolors.nominal-exact-real-lift.ascii-ssa.v1\0"
RELEASE_SHA256 = "2c626d8ee60eeb62ae4db53660d61bbc25e0efd4e557f0dc1e77565c130b6e52"

_UNARY = frozenset(("root3", "sqrt", "exp", "log", "sin", "cos", "abs", "sign"))
_BINARY = frozenset(("add", "sub", "mul", "div", "min", "max", "pow_pos", "pow_nn", "ratio0"))
_EXPECTED = {
    "point": (
        (("r8", "u8"), ("g8", "u8"), ("b8", "u8"),
         ("adapting_luminance", "real"), ("background_ratio", "real"),
         ("surround", "surround_profile")),
        226,
        ("jp", "ap", "bp"),
        39,
    ),
    "segment": (tuple((name, "real") for name in (
        "segment_t", "segment_a", "segment_b", "segment_t0", "segment_t1",
        "segment_c0a", "segment_c0b", "segment_c1a", "segment_c1b",
        "segment_rho0", "segment_rho1", "segment_g00", "segment_g01", "segment_g11",
    )), 27, ("segment_f",), 0),
    "singleton": (tuple((name, "real") for name in (
        "singleton_a", "singleton_b", "singleton_ca", "singleton_cb",
        "singleton_rho", "singleton_g00", "singleton_g01", "singleton_g11",
    )), 12, ("singleton_f",), 0),
}


class FormulaError(ValueError):
    pass


@dataclass(frozen=True)
class Node:
    name: str
    result: str
    operator: str
    arguments: tuple[str, ...]


@dataclass(frozen=True)
class Program:
    name: str
    inputs: tuple[tuple[str, str], ...]
    nodes: tuple[Node, ...]
    outputs: tuple[str, ...]


@dataclass(frozen=True)
class Formula:
    decode: tuple[int, ...]
    literals: tuple[tuple[str, int], ...]
    programs: tuple[Program, ...]


class Cursor:
    def __init__(self, lines: tuple[str, ...]):
        self.lines = lines
        self.index = 0

    def take(self) -> str:
        if self.index >= len(self.lines):
            raise FormulaError(f"unexpected end at line {self.index + 1}")
        value = self.lines[self.index]
        self.index += 1
        return value

    def expect(self, value: str) -> None:
        actual = self.take()
        if actual != value:
            raise FormulaError(f"expected {value!r}, got {actual!r}")


def _record(line: str, count: int) -> tuple[str, ...]:
    values = tuple(line.split(" "))
    if len(values) != count:
        raise FormulaError(f"record arity {len(values)} != {count}")
    return values


def _bits(value: str) -> int:
    if len(value) != 16 or any(ch not in "0123456789abcdef" for ch in value):
        raise FormulaError("noncanonical binary64 payload")
    bits = int(value, 16)
    if bits & 0x7FF0000000000000 == 0x7FF0000000000000:
        raise FormulaError("nonfinite binary64 payload")
    if bits == 0x8000000000000000:
        raise FormulaError("negative zero")
    return bits


def _node_type_check(node: Node, symbols: dict[str, str]) -> None:
    try:
        argument_types = tuple(symbols[name] for name in node.arguments)
    except KeyError as error:
        raise FormulaError(f"unknown or forward reference {error.args[0]}") from None
    if node.operator == "lookup":
        valid = node.result == "real" and argument_types == ("decode_table", "u8")
    elif node.operator == "eq":
        valid = node.result == "bool" and len(argument_types) == 2 and argument_types[0] == argument_types[1] == "surround_profile"
    elif node.operator == "select":
        valid = len(argument_types) == 3 and argument_types[0] == "bool" and argument_types[1] == argument_types[2] == node.result == "real"
    elif node.operator in _UNARY:
        valid = node.result == "real" and argument_types == ("real",)
    elif node.operator in _BINARY:
        valid = node.result == "real" and argument_types == ("real", "real")
    else:
        valid = False
    if not valid:
        raise FormulaError(f"operator/type mismatch for {node.name}")


def _program(cursor: Cursor, name: str, globals_: dict[str, str]) -> Program:
    expected_inputs, expected_nodes, expected_outputs, checkpoints = _EXPECTED[name]
    cursor.expect(f"{name}_inputs {len(expected_inputs)}")
    symbols = dict(globals_)
    inputs: list[tuple[str, str]] = []
    for expected in expected_inputs:
        record = _record(cursor.take(), 3)
        if record != ("input", *expected) or record[1] in symbols:
            raise FormulaError(f"foreign {name} input")
        symbols[record[1]] = record[2]
        inputs.append((record[1], record[2]))
    cursor.expect(f"{name}_nodes {expected_nodes}")
    nodes: list[Node] = []
    for _ in range(expected_nodes):
        record = tuple(cursor.take().split(" "))
        if len(record) < 5 or record[0] != "node" or not record[1].islower():
            raise FormulaError("invalid node")
        node = Node(record[1], record[2], record[3], record[4:])
        _node_type_check(node, symbols)
        if node.name in symbols:
            raise FormulaError("shadowed node")
        symbols[node.name] = node.result
        nodes.append(node)
    if checkpoints:
        cursor.expect(f"{name}_checkpoints {checkpoints}")
        for _ in range(checkpoints):
            checkpoint = _record(cursor.take(), 3)
            if checkpoint[0] != "checkpoint" or checkpoint[2] not in {node.name for node in nodes}:
                raise FormulaError("invalid checkpoint")
    cursor.expect(f"{name}_outputs {len(expected_outputs)}")
    outputs: list[str] = []
    for expected in expected_outputs:
        record = _record(cursor.take(), 3)
        if record != ("output", expected, "real") or symbols.get(expected) != "real":
            raise FormulaError(f"foreign {name} output")
        outputs.append(expected)
    return Program(name, tuple(inputs), tuple(nodes), tuple(outputs))


def parse(source: bytes) -> Formula:
    if hashlib.sha256(source).hexdigest() != SOURCE_SHA256:
        raise FormulaError("formula source is not the registered V1 content")
    release = hashlib.sha256(RELEASE_DOMAIN + len(source).to_bytes(8, "big") + source).hexdigest()
    if release != RELEASE_SHA256:
        raise FormulaError("formula release mismatch")
    if not source.isascii() or not source.endswith(b"\n") or source.endswith(b"\n\n"):
        raise FormulaError("formula is not canonical ASCII with one final LF")
    lines = tuple(source.decode("ascii")[:-1].split("\n"))
    if any(not line or line.startswith(" ") or line.endswith(" ") or "  " in line or "\t" in line or "\r" in line or "#" in line for line in lines):
        raise FormulaError("formula contains a noncanonical line")
    cursor = Cursor(lines)
    cursor.expect("labcolors_exact_real_ssa 1")
    cursor.expect("arithmetic exact_real_v1")
    cursor.expect("types 4")
    for declaration in ("type u8 unsigned_integer_0_255", "type real mathematical_real", "type bool exact_boolean", "type surround_profile closed_enum"):
        cursor.expect(declaration)
    cursor.expect("operators 20")
    operators = tuple(cursor.take() for _ in range(20))
    if operators != (
        "operator lookup 2 real table_u8_exact_dyadic_at_ordinal",
        "operator eq 2 bool exact_same_type_equality",
        "operator select 3 same bool_true_second_else_third",
        "operator add 2 real exact_x_plus_y",
        "operator sub 2 real exact_x_minus_y",
        "operator mul 2 real exact_x_times_y",
        "operator div 2 real domain_y_ne_zero_x_div_y_else_domain_unproven",
        "operator min 2 real exact_lesser_real",
        "operator max 2 real exact_greater_real",
        "operator root3 1 real domain_x_ge_zero_unique_y_ge_zero_y_cubed_eq_x_else_domain_unproven",
        "operator sqrt 1 real domain_x_ge_zero_unique_y_ge_zero_y_squared_eq_x_else_domain_unproven",
        "operator exp 1 real analytic_natural_exponential",
        "operator log 1 real domain_x_gt_zero_analytic_natural_logarithm_else_domain_unproven",
        "operator sin 1 real analytic_sine_radians",
        "operator cos 1 real analytic_cosine_radians",
        "operator abs 1 real exact_absolute_value",
        "operator sign 1 real negative_minus_one_zero_zero_positive_one",
        "operator pow_pos 2 real domain_x_gt_zero_exp_y_mul_log_x_else_domain_unproven",
        "operator pow_nn 2 real if_x_eq_zero_and_y_gt_zero_zero_else_pow_pos",
        "operator ratio0 2 real if_x_eq_zero_and_y_eq_zero_zero_else_domain_y_gt_zero_x_div_y",
    ):
        raise FormulaError("operator contract drift")
    cursor.expect("decode_table decode_srgb8 256")
    decode: list[int] = []
    for ordinal in range(256):
        record = _record(cursor.take(), 3)
        if record[:2] != ("decode", f"{ordinal:02x}"):
            raise FormulaError("decode order drift")
        decode.append(_bits(record[2]))
    cursor.expect("literals 56")
    literals: list[tuple[str, int]] = []
    names: set[str] = set()
    values: set[int] = set()
    for _ in range(56):
        record = _record(cursor.take(), 3)
        bits = _bits(record[2])
        if record[0] != "literal" or not record[1].islower() or record[1] in names or bits in values:
            raise FormulaError("invalid literal")
        names.add(record[1])
        values.add(bits)
        literals.append((record[1], bits))
    cursor.expect("enum_type surround_profile 3")
    enums = (("surround_average", 1), ("surround_dim", 2), ("surround_dark", 3))
    for name, tag in enums:
        if _record(cursor.take(), 4) != ("enum", "surround_profile", name, f"{tag:02x}"):
            raise FormulaError("surround enum drift")
    globals_ = {"decode_srgb8": "decode_table"}
    globals_.update({name: "real" for name, _ in literals})
    globals_.update({name: "surround_profile" for name, _ in enums})
    programs = tuple(_program(cursor, name, globals_) for name in ("point", "segment", "singleton"))
    cursor.expect("driver 6")
    for rule in ("rule tone_domain closed_first_last", "rule out_of_tone_domain outside", "rule one_knot_tone exact_equality_required", "rule one_knot_predicate singleton_f_le_zero", "rule multi_knot_predicate piecewise_linear_segment_f_le_zero", "rule boundary inclusive"):
        cursor.expect(rule)
    cursor.expect("end")
    if cursor.index != len(lines):
        raise FormulaError("trailing records")
    return Formula(tuple(decode), tuple(literals), programs)


def _real_expr(name: str, slots: dict[str, int]) -> str:
    return f"real + {slots[name]}"


def _emit_program(formula: Formula, program: Program) -> list[str]:
    slots: dict[str, int] = {name: index for index, (name, _bits_value) in enumerate(formula.literals)}
    surround = {"surround_average": "1", "surround_dim": "2", "surround_dark": "3"}
    booleans: dict[str, str] = {}
    for name, kind in program.inputs:
        if kind == "real":
            slots[name] = len(slots)
        elif kind == "surround_profile":
            surround[name] = "surround"
    for node in program.nodes:
        if node.result == "real":
            slots[node.name] = len(slots)
        else:
            booleans[node.name] = f"condition_{len(booleans)}"
    signatures = {
        "point": "lc_mpfi_status lc_mpfi_formula_point(mpfi_ptr output, const uint8_t rgb[3], mpfi_srcptr context, uint8_t surround)",
        "segment": "lc_mpfi_status lc_mpfi_formula_segment(mpfi_ptr output, mpfi_srcptr input)",
        "singleton": "lc_mpfi_status lc_mpfi_formula_singleton(mpfi_ptr output, mpfi_srcptr input)",
    }
    lines = [signatures[program.name], "{", "    lc_mpfi_status status = LC_MPFI_OK;", f"    __mpfi_struct real[{len(slots)}];"]
    lines.extend(f"    mpfi_init2(real + {index}, mpfi_get_prec(output));" for index in range(len(slots)))
    for name, bits in formula.literals:
        lines.append(f"    status = lc_mpfi_set_dyadic_bits(real + {slots[name]}, UINT64_C(0x{bits:016x}));")
        lines.append("    if (status != LC_MPFI_OK) goto cleanup;")
    real_cursor = 0
    u8_cursor = 0
    u8_values: dict[str, str] = {}
    for name, kind in program.inputs:
        if kind == "real":
            source = "context" if program.name == "point" else "input"
            lines.append(f"    mpfi_set(real + {slots[name]}, {source} + {real_cursor});")
            real_cursor += 1
        elif kind == "u8":
            u8_values[name] = f"rgb[{u8_cursor}]"
            u8_cursor += 1
    adapters = {
        "add": "lc_mpfi_add", "sub": "lc_mpfi_sub", "mul": "lc_mpfi_mul", "div": "lc_mpfi_div",
        "min": "lc_mpfi_min", "max": "lc_mpfi_max", "root3": "lc_mpfi_root3", "sqrt": "lc_mpfi_sqrt",
        "exp": "lc_mpfi_exp", "log": "lc_mpfi_log", "sin": "lc_mpfi_sin", "cos": "lc_mpfi_cos",
        "abs": "lc_mpfi_abs", "sign": "lc_mpfi_sign", "pow_pos": "lc_mpfi_pow_pos",
        "pow_nn": "lc_mpfi_pow_nn", "ratio0": "lc_mpfi_ratio0",
    }
    for node in program.nodes:
        target = _real_expr(node.name, slots) if node.result == "real" else ""
        if node.operator == "lookup":
            lines.append(f"    status = lc_mpfi_set_dyadic_bits({target}, LC_MPFI_DECODE_BITS[(size_t){u8_values[node.arguments[1]]}]);")
            lines.append("    if (status != LC_MPFI_OK) goto cleanup;")
        elif node.operator == "eq":
            lines.append(f"    int {booleans[node.name]} = ({surround[node.arguments[0]]} == {surround[node.arguments[1]]});")
        elif node.operator == "select":
            condition = booleans[node.arguments[0]]
            lines.append(f"    mpfi_set({target}, {condition} ? {_real_expr(node.arguments[1], slots)} : {_real_expr(node.arguments[2], slots)});")
        else:
            arguments = ", ".join(_real_expr(argument, slots) for argument in node.arguments)
            lines.append(f"    status = {adapters[node.operator]}({target}, {arguments});")
            lines.append("    if (status != LC_MPFI_OK) goto cleanup;")
    for index, name in enumerate(program.outputs):
        destination = f"output + {index}" if len(program.outputs) > 1 else "output"
        lines.append(f"    mpfi_set({destination}, {_real_expr(name, slots)});")
    lines.append("cleanup:")
    lines.extend(f"    mpfi_clear(real + {index});" for index in range(len(slots) - 1, -1, -1))
    lines.extend(("    return status;", "}", ""))
    return lines


def emit(formula: Formula) -> bytes:
    lines = [
        "/* Generated from the registered exact-real SSA; do not edit. */",
        "#include <stddef.h>",
        "#include <stdint.h>",
        "#include \"formula.h\"",
        "",
        "static const uint64_t LC_MPFI_DECODE_BITS[256] = {",
    ]
    for index in range(0, 256, 4):
        values = ", ".join(f"UINT64_C(0x{value:016x})" for value in formula.decode[index : index + 4])
        lines.append(f"    {values},")
    lines.append("};")
    lines.append("")
    for program in formula.programs:
        lines.extend(_emit_program(formula, program))
    return ("\n".join(lines) + "\n").encode("ascii")


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: formula.py FORMULA", file=sys.stderr)
        return 2
    try:
        output = emit(parse(Path(argv[1]).read_bytes()))
    except (OSError, FormulaError) as error:
        print(f"formula rejected: {error}", file=sys.stderr)
        return 1
    sys.stdout.buffer.write(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
