"""Independent strict interpreter for the V1 exact-real SSA.

The third verifier must not trust the engine's parser or code generator:
this module re-parses the immutable formula bytes with its own strict reader
and evaluates programs over rigorous interval values.  Only the protocol
release digest pins the accepted content, recomputed here from the declared
domain label.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass
from fractions import Fraction
from functools import cached_property

import region_proof_protocol as protocol

from . import intervalmath


class SemanticFormulaError(ValueError):
    """The formula bytes are not the registered V1 exact-real SSA."""


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


POINT_DYNAMIC_INPUTS_V1 = ("r8", "g8", "b8")

FOLD_SHARED_DOMAIN_V1 = b"labcolors.proof-region.folded-point-shared.v1\0"


def shared_inputs_fingerprint_v1(
    program_inputs: tuple[tuple[str, str], ...],
    shared_inputs: dict[str, object],
) -> bytes:
    """Canonical identity of the job-shared fold configuration.

    A fold's static constants are derived from exactly these bindings; the
    fingerprint makes a fold computed for a foreign configuration detectable
    instead of silently replaying stale constants against another job.
    """

    hasher = hashlib.sha256()
    hasher.update(FOLD_SHARED_DOMAIN_V1)
    for name, kind in program_inputs:
        if name in POINT_DYNAMIC_INPUTS_V1:
            continue
        value = shared_inputs[name]
        if kind == "real":
            lo = value.lo
            hi = value.hi
            payload = (
                f"{name}:real:{lo.numerator}:{lo.denominator}"
                f":{hi.numerator}:{hi.denominator}"
            )
        else:
            payload = f"{name}:int:{value}"
        hasher.update(payload.encode("ascii"))
    return hasher.digest()


@dataclass(frozen=True)
class FoldedPointProgramV1:
    """The point program partially evaluated over job-shared inputs.

    Every node whose operands are fixed by the literal, enum and shared
    environment is evaluated exactly once under the folding context's
    precision discipline; only the point-dependent suffix is retained, in
    program order, so each point replays bit-identical interval semantics.
    A fold never leaves the evaluation context that computed it: its guard
    and cap bits are part of the replayed decision semantics.
    """

    guard_bits: int
    cap_bits: int
    shared_fingerprint: bytes
    static_names: tuple[str, ...]
    static_environment: tuple[tuple[str, object], ...]
    dynamic_nodes: tuple[SemanticNode, ...]


@dataclass(frozen=True)
class SemanticNode:
    name: str
    result: str
    operator: str
    arguments: tuple[str, ...]


@dataclass(frozen=True)
class SemanticProgram:
    name: str
    inputs: tuple[tuple[str, str], ...]
    nodes: tuple[SemanticNode, ...]
    outputs: tuple[str, ...]


@dataclass(frozen=True)
class SemanticFormula:
    decode: tuple[Fraction, ...]
    literals: tuple[tuple[str, int], ...]
    enums: tuple[tuple[str, int], ...]
    programs: tuple[SemanticProgram, ...]

    def program(self, name: str) -> SemanticProgram:
        for program in self.programs:
            if program.name == name:
                return program
        raise SemanticFormulaError(f"missing program {name}")


class _Lines:
    def __init__(self, values: list[str]):
        self.values = values
        self.cursor = 0

    def next(self) -> str:
        if self.cursor >= len(self.values):
            raise SemanticFormulaError(f"unexpected end at line {self.cursor + 1}")
        value = self.values[self.cursor]
        self.cursor += 1
        return value

    def expect(self, expected: str) -> None:
        actual = self.next()
        if actual != expected:
            raise SemanticFormulaError(
                f"line {self.cursor}: expected {expected!r}, got {actual!r}"
            )


def _identifier(value: str) -> bool:
    return bool(value) and value[0].islower() and all(
        byte.islower() or byte.isdigit() or byte == "_" for byte in value
    )


def _fields(line: str, count: int) -> tuple[str, ...]:
    result = tuple(line.split(" "))
    if len(result) != count:
        raise SemanticFormulaError(f"record arity {len(result)} != {count}")
    return result


def binary64_to_fraction(bits: int) -> Fraction:
    """Exact binary64 value as a dyadic Fraction; rejects nonfinite payloads."""

    if bits & 0x7FF0_0000_0000_0000 == 0x7FF0_0000_0000_0000:
        raise SemanticFormulaError("nonfinite binary64 payload")
    if bits == 0x8000_0000_0000_0000:
        raise SemanticFormulaError("negative zero")
    sign = -1 if bits >> 63 else 1
    exponent = (bits >> 52) & 0x7FF
    fraction = bits & ((1 << 52) - 1)
    if exponent == 0:
        significand = fraction
        power = -1074
    else:
        significand = (1 << 52) | fraction
        power = exponent - 1075
    numerator = sign * significand
    if power >= 0:
        return Fraction(numerator << power, 1)
    return Fraction(numerator, 1 << -power)


def _finite_bits(token: str) -> int:
    if len(token) != 16 or any(byte not in "0123456789abcdef" for byte in token):
        raise SemanticFormulaError("noncanonical binary64 payload")
    return int(token, 16)


def _validate_node(node: SemanticNode, symbols: dict[str, str]) -> None:
    try:
        types = tuple(symbols[value] for value in node.arguments)
    except KeyError as error:
        raise SemanticFormulaError(
            f"unknown or forward reference {error.args[0]}"
        ) from None
    unary = {"root3", "sqrt", "exp", "log", "sin", "cos", "abs", "sign"}
    binary = {"add", "sub", "mul", "div", "min", "max", "pow_pos", "pow_nn", "ratio0"}
    if node.operator == "lookup":
        valid = node.result == "real" and types == ("decode_table", "u8")
    elif node.operator == "eq":
        # Equality over real values is undecidable on intervals; the parser
        # admits discrete operands only, which matches the registered V1
        # formula (it compares the surround enum exclusively).
        valid = (
            node.result == "bool"
            and len(types) == 2
            and types[0] == types[1]
            and types[0] in ("u8", "surround_profile", "bool")
        )
    elif node.operator == "select":
        valid = len(types) == 3 and types[0] == "bool" and types[1] == types[2] == node.result
    elif node.operator in unary:
        valid = node.result == "real" and types == ("real",)
    elif node.operator in binary:
        valid = node.result == "real" and types == ("real", "real")
    else:
        valid = False
    if not valid:
        raise SemanticFormulaError(f"operator/type mismatch for {node.name}")


def _parse_program(lines: _Lines, name: str, globals_: dict[str, str]) -> SemanticProgram:
    expected_inputs, expected_nodes, expected_outputs = PROGRAM_INTERFACES[name]
    lines.expect(f"{name}_inputs {len(expected_inputs)}")
    symbols = dict(globals_)
    inputs: list[tuple[str, str]] = []
    for expected in expected_inputs:
        record = _fields(lines.next(), 3)
        if record != ("input", *expected):
            raise SemanticFormulaError(f"foreign {name} input")
        if record[1] in symbols:
            raise SemanticFormulaError("shadowed input")
        symbols[record[1]] = record[2]
        inputs.append((record[1], record[2]))

    lines.expect(f"{name}_nodes {expected_nodes}")
    nodes: list[SemanticNode] = []
    for _ in range(expected_nodes):
        record = tuple(lines.next().split(" "))
        if len(record) < 5 or record[0] != "node" or not _identifier(record[1]):
            raise SemanticFormulaError("invalid node")
        node = SemanticNode(record[1], record[2], record[3], record[4:])
        _validate_node(node, symbols)
        if node.name in symbols:
            raise SemanticFormulaError("shadowed node")
        symbols[node.name] = node.result
        nodes.append(node)

    if name == "point":
        lines.expect("point_checkpoints 39")
        checkpoint_names: set[str] = set()
        node_names = {node.name for node in nodes}
        for _ in range(39):
            record = _fields(lines.next(), 3)
            if (
                record[0] != "checkpoint"
                or record[1] in checkpoint_names
                or record[2] not in node_names
                or symbols[record[2]] != "real"
            ):
                raise SemanticFormulaError("invalid checkpoint")
            checkpoint_names.add(record[1])

    lines.expect(f"{name}_outputs {len(expected_outputs)}")
    outputs: list[str] = []
    for expected in expected_outputs:
        record = _fields(lines.next(), 3)
        if record != ("output", expected, "real") or symbols.get(expected) != "real":
            raise SemanticFormulaError("foreign output")
        outputs.append(expected)
    return SemanticProgram(name, tuple(inputs), tuple(nodes), tuple(outputs))


def parse_formula(source: bytes) -> SemanticFormula:
    """Strictly re-parse the immutable V1 formula bytes."""

    if type(source) is not bytes:
        raise SemanticFormulaError("formula source must be owned bytes")
    release = hashlib.sha256(
        protocol.FORMULA_RELEASE_DOMAIN_V1
        + len(source).to_bytes(8, "big")
        + source
    ).hexdigest()
    if release != protocol.FORMULA_RELEASE_V1.hex():
        raise SemanticFormulaError("formula release mismatch")
    if not source.isascii() or not source.endswith(b"\n") or source.endswith(b"\n\n"):
        raise SemanticFormulaError("formula is not canonical ASCII with one final LF")
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
            raise SemanticFormulaError(f"line {index} is not canonical")

    lines = _Lines(values)
    lines.expect("labcolors_exact_real_ssa 1")
    lines.expect("arithmetic exact_real_v1")
    lines.expect(f"types {len(TYPE_DECLARATIONS)}")
    for declaration in TYPE_DECLARATIONS:
        lines.expect(declaration)
    lines.expect(f"operators {len(OPERATOR_DECLARATIONS)}")
    for declaration in OPERATOR_DECLARATIONS:
        lines.expect(declaration)

    lines.expect("decode_table decode_srgb8 256")
    decode: list[Fraction] = []
    for ordinal in range(256):
        record = _fields(lines.next(), 3)
        if record[:2] != ("decode", f"{ordinal:02x}"):
            raise SemanticFormulaError("decode order drift")
        decode.append(binary64_to_fraction(_finite_bits(record[2])))

    lines.expect("literals 56")
    literals: list[tuple[str, int]] = []
    literal_names: set[str] = set()
    literal_values: set[int] = set()
    for _ in range(56):
        record = _fields(lines.next(), 3)
        bits = _finite_bits(record[2])
        if (
            record[0] != "literal"
            or not _identifier(record[1])
            or record[1] in literal_names
            or bits in literal_values
        ):
            raise SemanticFormulaError("invalid literal")
        literal_names.add(record[1])
        literal_values.add(bits)
        literals.append((record[1], bits))

    lines.expect("enum_type surround_profile 3")
    enums: list[tuple[str, int]] = []
    for name, tag in (("surround_average", 1), ("surround_dim", 2), ("surround_dark", 3)):
        record = _fields(lines.next(), 4)
        if record != ("enum", "surround_profile", name, f"{tag:02x}"):
            raise SemanticFormulaError("foreign surround enum")
        enums.append((name, tag))

    globals_: dict[str, str] = {"decode_srgb8": "decode_table"}
    globals_.update((name, "real") for name, _ in literals)
    globals_.update((name, "surround_profile") for name, _ in enums)
    programs = tuple(_parse_program(lines, name, globals_) for name in PROGRAM_INTERFACES)
    lines.expect(f"driver {len(DRIVER_RULES)}")
    for rule in DRIVER_RULES:
        lines.expect(rule)
    lines.expect("end")
    if lines.cursor != len(lines.values):
        raise SemanticFormulaError("trailing records")
    return SemanticFormula(tuple(decode), tuple(literals), tuple(enums), programs)


@dataclass(frozen=True)
class EvaluationContext:
    """Fixed precision policy for one replay rung."""

    formula: SemanticFormula
    guard_bits: int
    cap_bits: int

    def evaluate(
        self,
        program: SemanticProgram,
        inputs: dict[str, object],
    ) -> dict[str, object]:
        """Run one program; real values are intervals, u8/enum/bool are ints."""

        environment: dict[str, object] = dict(self._literal_environment)
        for name, kind in program.inputs:
            if name not in inputs:
                raise SemanticFormulaError(f"missing input {name}")
            value = inputs[name]
            if kind == "real":
                if type(value) is not intervalmath.Interval:
                    raise SemanticFormulaError(f"input {name} must be an interval")
            elif kind in ("u8", "surround_profile"):
                if type(value) is not int:
                    raise SemanticFormulaError(f"input {name} must be an integer")
            environment[name] = value

        for node in program.nodes:
            environment[node.name] = self._evaluate_node(node, environment)
        outputs: dict[str, object] = {}
        for name in program.outputs:
            value = environment[name]
            if type(value) is not intervalmath.Interval:
                raise SemanticFormulaError(f"output {name} is not a real value")
            outputs[name] = value
        return outputs

    def fold_point_program(self, shared_inputs: dict[str, object]) -> FoldedPointProgramV1:
        """Partially evaluate the point program over the job-shared inputs.

        A full-domain replay lifts 2^24 points under one shared context; the
        nodes already fixed by the literal, enum and shared environment are
        evaluated exactly once per rung here, leaving only the point-dependent
        suffix for the per-point replay.  The fold inherits this context's
        precision discipline and never outlives it.
        """

        program = self.formula.program("point")
        shared_names = self._validate_shared_inputs(program, shared_inputs)
        environment: dict[str, object] = dict(self._literal_environment)
        for name, _ in shared_names:
            environment[name] = shared_inputs[name]

        static_names: list[str] = []
        static_environment: list[tuple[str, object]] = []
        dynamic_nodes: list[SemanticNode] = []
        dynamic_names: set[str] = set(POINT_DYNAMIC_INPUTS_V1)
        for node in program.nodes:
            # The lookup table name is release-pinned, not an environment
            # binding; only the index operand participates in the closure.
            bound = node.arguments[1:] if node.operator == "lookup" else node.arguments
            missing = tuple(name for name in bound if name not in environment)
            if not missing:
                value = self._evaluate_node(node, environment)
                environment[node.name] = value
                static_names.append(node.name)
                static_environment.append((node.name, value))
                continue
            if any(name not in dynamic_names for name in missing):
                raise SemanticFormulaError(
                    f"fold met an undeclared binding in {node.name}"
                )
            dynamic_nodes.append(node)
            dynamic_names.add(node.name)
        return FoldedPointProgramV1(
            guard_bits=self.guard_bits,
            cap_bits=self.cap_bits,
            shared_fingerprint=shared_inputs_fingerprint_v1(
                program.inputs, shared_inputs
            ),
            static_names=tuple(static_names),
            static_environment=tuple(static_environment),
            dynamic_nodes=tuple(dynamic_nodes),
        )

    def _validate_shared_inputs(
        self,
        program: SemanticProgram,
        shared_inputs: dict[str, object],
    ) -> tuple[tuple[str, str], ...]:
        shared_names = tuple(
            (name, kind)
            for name, kind in program.inputs
            if name not in POINT_DYNAMIC_INPUTS_V1
        )
        if set(shared_inputs) != {name for name, _ in shared_names}:
            raise SemanticFormulaError(
                "fold requires exactly the job-shared point inputs"
            )
        for name, kind in shared_names:
            value = shared_inputs[name]
            if kind == "real":
                if type(value) is not intervalmath.Interval:
                    raise SemanticFormulaError(f"shared input {name} must be an interval")
            elif type(value) is not int:
                raise SemanticFormulaError(f"shared input {name} must be an integer")
        return shared_names

    def evaluate_folded_point(
        self,
        folded: FoldedPointProgramV1,
        shared_inputs: dict[str, object],
        r8: int,
        g8: int,
        b8: int,
    ) -> dict[str, object]:
        """Replay the folded dynamic suffix for one sRGB8 point.

        The caller redeclares the shared configuration at evaluation time and
        the fold must carry the matching fingerprint: a fold computed for a
        foreign job context is rejected instead of replaying stale constants.
        """

        if folded.guard_bits != self.guard_bits or folded.cap_bits != self.cap_bits:
            raise SemanticFormulaError(
                "fold was computed under a different precision discipline"
            )
        program = self.formula.program("point")
        self._validate_shared_inputs(program, shared_inputs)
        if folded.shared_fingerprint != shared_inputs_fingerprint_v1(
            program.inputs, shared_inputs
        ):
            raise SemanticFormulaError(
                "fold was computed for a foreign shared configuration"
            )
        for name, value in (("r8", r8), ("g8", g8), ("b8", b8)):
            if type(value) is not int or value < 0 or value > 255:
                raise SemanticFormulaError(f"point input {name} must be an sRGB8 sample")
        # Release-pinned literals and enum tags stay implicit: they belong to
        # this context's formula, not to the fold's shared-input constants.
        environment: dict[str, object] = dict(self._literal_environment)
        environment.update(folded.static_environment)
        environment["r8"] = r8
        environment["g8"] = g8
        environment["b8"] = b8
        for node in folded.dynamic_nodes:
            environment[node.name] = self._evaluate_node(node, environment)
        outputs: dict[str, object] = {}
        for name in program.outputs:
            value = environment.get(name)
            if type(value) is not intervalmath.Interval:
                raise SemanticFormulaError(f"output {name} is not a real value")
            outputs[name] = value
        return outputs

    @cached_property
    def _literal_environment(self) -> dict[str, object]:
        # Literal and enum bindings are pinned by the release digest; decoding
        # them once instead of once per point (2^24+ evaluations per full
        # domain) removes a quadratic-fraction hot path from the replay.
        environment: dict[str, object] = {}
        for name, bits in self.formula.literals:
            environment[name] = intervalmath.exact(binary64_to_fraction(bits))
        for name, tag in self.formula.enums:
            environment[name] = tag
        return environment

    def _evaluate_node(
        self,
        node: SemanticNode,
        environment: dict[str, object],
    ) -> object:
        operator = node.operator
        guard = self.guard_bits
        cap = self.cap_bits
        if operator == "lookup":
            if node.arguments[0] != "decode_srgb8":
                raise SemanticFormulaError("lookup against foreign table")
            index = environment[node.arguments[1]]
            if type(index) is not int or index < 0 or index >= len(self.formula.decode):
                raise SemanticFormulaError("lookup index outside the decode table")
            return intervalmath.exact(self.formula.decode[index])
        arguments = tuple(environment[name] for name in node.arguments)
        if operator == "eq":
            left, right = arguments
            if type(left) is not int or type(right) is not int:
                raise SemanticFormulaError("equality over non-discrete values")
            return 1 if left == right else 0
        if operator == "select":
            condition, chosen, fallback = arguments
            if type(condition) is not int:
                raise SemanticFormulaError("select over non-discrete condition")
            return chosen if condition else fallback
        if operator == "add":
            return intervalmath.add(arguments[0], arguments[1])
        if operator == "sub":
            return intervalmath.sub(arguments[0], arguments[1])
        if operator == "mul":
            return intervalmath.mul(arguments[0], arguments[1])
        if operator == "div":
            return intervalmath.div(arguments[0], arguments[1], cap_bits=cap)
        if operator == "min":
            return intervalmath.minimum(arguments[0], arguments[1])
        if operator == "max":
            return intervalmath.maximum(arguments[0], arguments[1])
        if operator == "root3":
            return intervalmath.root3(arguments[0], guard_bits=guard, cap_bits=cap)
        if operator == "sqrt":
            return intervalmath.sqrt(arguments[0], guard_bits=guard, cap_bits=cap)
        if operator == "exp":
            return intervalmath.exp(arguments[0], guard_bits=guard, cap_bits=cap)
        if operator == "log":
            return intervalmath.log(arguments[0], guard_bits=guard, cap_bits=cap)
        if operator == "sin":
            return intervalmath.sin(arguments[0], guard_bits=guard, cap_bits=cap)
        if operator == "cos":
            return intervalmath.cos(arguments[0], guard_bits=guard, cap_bits=cap)
        if operator == "abs":
            return intervalmath.absolute(arguments[0])
        if operator == "sign":
            return intervalmath.sign(arguments[0])
        if operator == "pow_pos":
            return intervalmath.pow_pos(
                arguments[0], arguments[1], guard_bits=guard, cap_bits=cap
            )
        if operator == "pow_nn":
            return intervalmath.pow_nn(
                arguments[0], arguments[1], guard_bits=guard, cap_bits=cap
            )
        if operator == "ratio0":
            return intervalmath.ratio0(arguments[0], arguments[1], cap_bits=cap)
        raise SemanticFormulaError(f"foreign operator {operator}")
