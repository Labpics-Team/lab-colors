#!/usr/bin/env python3
"""Generate the Arb V1 evaluator from the immutable exact-real SSA."""

from __future__ import annotations

import hashlib
import sys
from dataclasses import dataclass
from pathlib import Path


SOURCE_SHA256 = "a6f77ac462f226453b1c27bbd8637b62780b9a640c317a6f50028dacd1de8540"
RELEASE_DOMAIN = b"labcolors.nominal-exact-real-lift.ascii-ssa.v1\0"
RELEASE_SHA256 = "2c626d8ee60eeb62ae4db53660d61bbc25e0efd4e557f0dc1e77565c130b6e52"

TYPE_DECLARATIONS = (
    "type u8 unsigned_integer_0_255",
    "type real mathematical_real",
    "type bool exact_boolean",
    "type surround_profile closed_enum",
)

OPERATOR_DECLARATIONS = (
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
)

DRIVER_RULES = (
    "rule tone_domain closed_first_last",
    "rule out_of_tone_domain outside",
    "rule one_knot_tone exact_equality_required",
    "rule one_knot_predicate singleton_f_le_zero",
    "rule multi_knot_predicate piecewise_linear_segment_f_le_zero",
    "rule boundary inclusive",
)

PROGRAM_INTERFACES = {
    "point": (
        (("r8", "u8"), ("g8", "u8"), ("b8", "u8"),
         ("adapting_luminance", "real"), ("background_ratio", "real"),
         ("surround", "surround_profile")),
        226,
        ("jp", "ap", "bp"),
    ),
    "segment": (
        tuple(
            (name, "real")
            for name in (
                "segment_t", "segment_a", "segment_b", "segment_t0",
                "segment_t1", "segment_c0a", "segment_c0b", "segment_c1a",
                "segment_c1b", "segment_rho0", "segment_rho1", "segment_g00",
                "segment_g01", "segment_g11",
            )
        ),
        27,
        ("segment_f",),
    ),
    "singleton": (
        tuple(
            (name, "real")
            for name in (
                "singleton_a", "singleton_b", "singleton_ca", "singleton_cb",
                "singleton_rho", "singleton_g00", "singleton_g01", "singleton_g11",
            )
        ),
        12,
        ("singleton_f",),
    ),
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
    enums: tuple[tuple[str, int], ...]
    programs: tuple[Program, ...]


class Lines:
    def __init__(self, values: list[str]):
        self.values = values
        self.cursor = 0

    def next(self) -> str:
        if self.cursor >= len(self.values):
            raise FormulaError(f"unexpected end at line {self.cursor + 1}")
        value = self.values[self.cursor]
        self.cursor += 1
        return value

    def expect(self, expected: str) -> None:
        actual = self.next()
        if actual != expected:
            raise FormulaError(
                f"line {self.cursor}: expected {expected!r}, got {actual!r}"
            )


def identifier(value: str) -> bool:
    return bool(value) and value[0].islower() and all(
        byte.islower() or byte.isdigit() or byte == "_" for byte in value
    )


def fields(line: str, count: int) -> tuple[str, ...]:
    result = tuple(line.split(" "))
    if len(result) != count:
        raise FormulaError(f"record arity {len(result)} != {count}")
    return result


def finite_bits(token: str) -> int:
    if len(token) != 16 or any(byte not in "0123456789abcdef" for byte in token):
        raise FormulaError("noncanonical binary64 payload")
    bits = int(token, 16)
    if bits & 0x7FF0_0000_0000_0000 == 0x7FF0_0000_0000_0000:
        raise FormulaError("nonfinite binary64 payload")
    if bits == 0x8000_0000_0000_0000:
        raise FormulaError("negative zero")
    return bits


def parse_program(lines: Lines, name: str, globals_: dict[str, str]) -> Program:
    expected_inputs, expected_nodes, expected_outputs = PROGRAM_INTERFACES[name]
    lines.expect(f"{name}_inputs {len(expected_inputs)}")
    symbols = dict(globals_)
    inputs: list[tuple[str, str]] = []
    for expected in expected_inputs:
        record = fields(lines.next(), 3)
        if record != ("input", *expected):
            raise FormulaError(f"foreign {name} input")
        if record[1] in symbols:
            raise FormulaError("shadowed input")
        symbols[record[1]] = record[2]
        inputs.append((record[1], record[2]))

    lines.expect(f"{name}_nodes {expected_nodes}")
    nodes: list[Node] = []
    for _ in range(expected_nodes):
        record = tuple(lines.next().split(" "))
        if len(record) < 5 or record[0] != "node" or not identifier(record[1]):
            raise FormulaError("invalid node")
        node = Node(record[1], record[2], record[3], record[4:])
        validate_node(node, symbols)
        if node.name in symbols:
            raise FormulaError("shadowed node")
        symbols[node.name] = node.result
        nodes.append(node)

    if name == "point":
        lines.expect("point_checkpoints 39")
        checkpoint_names: set[str] = set()
        node_names = {node.name for node in nodes}
        for _ in range(39):
            record = fields(lines.next(), 3)
            if (
                record[0] != "checkpoint"
                or record[1] in checkpoint_names
                or record[2] not in node_names
                or symbols[record[2]] != "real"
            ):
                raise FormulaError("invalid checkpoint")
            checkpoint_names.add(record[1])

    lines.expect(f"{name}_outputs {len(expected_outputs)}")
    outputs: list[str] = []
    for expected in expected_outputs:
        record = fields(lines.next(), 3)
        if record != ("output", expected, "real") or symbols.get(expected) != "real":
            raise FormulaError("foreign output")
        outputs.append(expected)
    return Program(name, tuple(inputs), tuple(nodes), tuple(outputs))


def validate_node(node: Node, symbols: dict[str, str]) -> None:
    try:
        types = tuple(symbols[value] for value in node.arguments)
    except KeyError as error:
        raise FormulaError(f"unknown or forward reference {error.args[0]}") from None
    unary = {"root3", "sqrt", "exp", "log", "sin", "cos", "abs", "sign"}
    binary = {"add", "sub", "mul", "div", "min", "max", "pow_pos", "pow_nn", "ratio0"}
    if node.operator == "lookup":
        valid = node.result == "real" and types == ("decode_table", "u8")
    elif node.operator == "eq":
        valid = node.result == "bool" and len(types) == 2 and types[0] == types[1] != "decode_table"
    elif node.operator == "select":
        valid = len(types) == 3 and types[0] == "bool" and types[1] == types[2] == node.result
    elif node.operator in unary:
        valid = node.result == "real" and types == ("real",)
    elif node.operator in binary:
        valid = node.result == "real" and types == ("real", "real")
    else:
        valid = False
    if not valid:
        raise FormulaError(f"operator/type mismatch for {node.name}")


def parse(source: bytes) -> Formula:
    if hashlib.sha256(source).hexdigest() != SOURCE_SHA256:
        raise FormulaError("formula source is not the registered V1 content")
    release = hashlib.sha256(
        RELEASE_DOMAIN + len(source).to_bytes(8, "big") + source
    ).hexdigest()
    if release != RELEASE_SHA256:
        raise FormulaError("formula release mismatch")
    if not source.isascii() or not source.endswith(b"\n") or source.endswith(b"\n\n"):
        raise FormulaError("formula is not canonical ASCII with one final LF")
    text = source.decode("ascii")[:-1]
    values = text.split("\n")
    for index, line in enumerate(values, 1):
        if (
            not line
            or line.startswith(" ")
            or line.endswith(" ")
            or "  " in line
            or "\t" in line
            or "\r" in line
            or "#" in line
        ):
            raise FormulaError(f"line {index} is not canonical")

    lines = Lines(values)
    lines.expect("labcolors_exact_real_ssa 1")
    lines.expect("arithmetic exact_real_v1")
    lines.expect(f"types {len(TYPE_DECLARATIONS)}")
    for declaration in TYPE_DECLARATIONS:
        lines.expect(declaration)
    lines.expect(f"operators {len(OPERATOR_DECLARATIONS)}")
    for declaration in OPERATOR_DECLARATIONS:
        lines.expect(declaration)

    lines.expect("decode_table decode_srgb8 256")
    decode: list[int] = []
    for ordinal in range(256):
        record = fields(lines.next(), 3)
        if record[:2] != ("decode", f"{ordinal:02x}"):
            raise FormulaError("decode order drift")
        decode.append(finite_bits(record[2]))

    lines.expect("literals 56")
    literals: list[tuple[str, int]] = []
    literal_names: set[str] = set()
    literal_values: set[int] = set()
    for _ in range(56):
        record = fields(lines.next(), 3)
        bits = finite_bits(record[2])
        if (
            record[0] != "literal"
            or not identifier(record[1])
            or record[1] in literal_names
            or bits in literal_values
        ):
            raise FormulaError("invalid literal")
        literal_names.add(record[1])
        literal_values.add(bits)
        literals.append((record[1], bits))

    lines.expect("enum_type surround_profile 3")
    enums: list[tuple[str, int]] = []
    for name, tag in (("surround_average", 1), ("surround_dim", 2), ("surround_dark", 3)):
        record = fields(lines.next(), 4)
        if record != ("enum", "surround_profile", name, f"{tag:02x}"):
            raise FormulaError("foreign surround enum")
        enums.append((name, tag))

    globals_: dict[str, str] = {"decode_srgb8": "decode_table"}
    globals_.update((name, "real") for name, _ in literals)
    globals_.update((name, "surround_profile") for name, _ in enums)
    programs = tuple(parse_program(lines, name, globals_) for name in PROGRAM_INTERFACES)
    lines.expect(f"driver {len(DRIVER_RULES)}")
    for rule in DRIVER_RULES:
        lines.expect(rule)
    lines.expect("end")
    if lines.cursor != len(lines.values):
        raise FormulaError("trailing records")
    return Formula(tuple(decode), tuple(literals), tuple(enums), programs)


def real_expression(name: str, real: dict[str, int]) -> str:
    return f"real + {real[name]}"


def emit_program(formula: Formula, program: Program) -> list[str]:
    real: dict[str, int] = {}
    surround: dict[str, str] = {name: str(tag) for name, tag in formula.enums}
    boolean: dict[str, str] = {}
    lines: list[str] = []

    for name, _ in formula.literals:
        real[name] = len(real)
    for name, kind in program.inputs:
        if kind == "real":
            real[name] = len(real)
        elif kind == "surround_profile":
            surround[name] = "surround"

    for node in program.nodes:
        if node.result == "real":
            real[node.name] = len(real)
        elif node.result == "bool":
            boolean[node.name] = f"condition_{len(boolean)}"

    signature = {
        "point": "lc_status lc_formula_point(arb_ptr output, const uint8_t rgb[3], arb_srcptr context, uint8_t surround, slong precision)",
        "segment": "lc_status lc_formula_segment(arb_t output, arb_srcptr input, slong precision)",
        "singleton": "lc_status lc_formula_singleton(arb_t output, arb_srcptr input, slong precision)",
    }[program.name]
    lines.extend((signature, "{", "    lc_status status = LC_OK;", f"    arb_struct real[{len(real)}];"))
    for index in range(len(real)):
        lines.append(f"    arb_init(real + {index});")
    for name, bits in formula.literals:
        lines.append(
            f"    status = lc_set_dyadic_bits(real + {real[name]}, UINT64_C(0x{bits:016x}));"
        )
        lines.append("    if (status != LC_OK) goto cleanup;")

    real_cursor = 0
    u8_cursor = 0
    u8_values: dict[str, str] = {}
    for name, kind in program.inputs:
        if kind == "real":
            lines.append(f"    arb_set(real + {real[name]}, {'context' if program.name == 'point' else 'input'} + {real_cursor});")
            real_cursor += 1
        elif kind == "u8":
            u8_values[name] = f"rgb[{u8_cursor}]"
            u8_cursor += 1

    adapter = {
        "add": "lc_add", "sub": "lc_sub", "mul": "lc_mul", "div": "lc_div",
        "min": "lc_min", "max": "lc_max", "root3": "lc_root3", "sqrt": "lc_sqrt",
        "exp": "lc_exp", "log": "lc_log", "sin": "lc_sin", "cos": "lc_cos",
        "abs": "lc_abs", "sign": "lc_sign", "pow_pos": "lc_pow_pos",
        "pow_nn": "lc_pow_nn", "ratio0": "lc_ratio0",
    }
    for node in program.nodes:
        target = real_expression(node.name, real) if node.result == "real" else ""
        if node.operator == "lookup":
            lines.append(
                f"    status = lc_set_dyadic_bits({target}, LC_DECODE_BITS[(size_t){u8_values[node.arguments[1]]}]);"
            )
            lines.append("    if (status != LC_OK) goto cleanup;")
        elif node.operator == "eq":
            left = surround[node.arguments[0]]
            right = surround[node.arguments[1]]
            lines.append(f"    int {boolean[node.name]} = ({left} == {right});")
        elif node.operator == "select":
            condition = boolean[node.arguments[0]]
            left = real_expression(node.arguments[1], real)
            right = real_expression(node.arguments[2], real)
            lines.append(f"    arb_set({target}, {condition} ? {left} : {right});")
        else:
            arguments = ", ".join(real_expression(name, real) for name in node.arguments)
            lines.append(
                f"    status = {adapter[node.operator]}({target}, {arguments}, precision);"
            )
            lines.append("    if (status != LC_OK) goto cleanup;")

    for index, name in enumerate(program.outputs):
        destination = f"output + {index}" if len(program.outputs) > 1 else "output"
        lines.append(f"    arb_set({destination}, {real_expression(name, real)});")
    lines.append("cleanup:")
    for index in range(len(real) - 1, -1, -1):
        lines.append(f"    arb_clear(real + {index});")
    lines.extend(("    return status;", "}", ""))
    return lines


def emit(formula: Formula) -> bytes:
    output = [
        "/* Generated from the registered exact-real SSA; do not edit. */",
        "#include <stddef.h>",
        "#include <stdint.h>",
        "#include \"formula.h\"",
        "",
        "static const uint64_t LC_DECODE_BITS[256] = {",
    ]
    for index in range(0, 256, 4):
        values = ", ".join(
            f"UINT64_C(0x{value:016x})" for value in formula.decode[index : index + 4]
        )
        output.append(f"    {values},")
    output.extend(("};", ""))
    for program in formula.programs:
        output.extend(emit_program(formula, program))
    return ("\n".join(output) + "\n").encode("ascii")


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: formula.py FORMULA", file=sys.stderr)
        return 2
    try:
        source = Path(argv[1]).read_bytes()
        generated = emit(parse(source))
    except (OSError, FormulaError) as error:
        print(f"formula rejected: {error}", file=sys.stderr)
        return 1
    sys.stdout.buffer.write(generated)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
