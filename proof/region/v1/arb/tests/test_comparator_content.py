#!/usr/bin/env python3
"""Закрепляет СОДЕРЖИМОЕ десяти координат Arb-компаратора.

Манифест связывал только *числа* координат — sha256 от преимиджа. Сам преимидж
не удерживал никто: домен-сепаратор внутри деривации можно было переписать, а
пару ``engine_release``/``upstream_source`` — поменять местами, и весь набор
оставался зелёным. Значение координаты переопределялось молча, хотя
идентичность компаратора — фундамент решающей цепи.

Модуль закрепляет ВХОД свёртки, а не её результат, тремя независимыми слоями:

1. Закон именования: координата с именем ``X`` обязана нести домен-сепаратор,
   который произносит ``X``. Ловит перестановку координат местами.
2. Reference-вектор: точные байты каждой source-bound координаты на
   детерминированном входе, закреплённые по ИМЕНИ координаты. Ловит любое
   изменение байтов, включая перестановку и правку сепаратора.
3. Структурный golden: разбор преимиджа на (сепаратор, версия, упорядоченные
   чанки) в форме, не содержащей байтов допущенных исходников. Правка любого
   сепаратора, числа или порядка чанков и длины короткого (≤64 Б) чанка дают
   читаемый diff; чанк длиннее 64 Б закреплён только присутствием и позицией,
   его байты и длина этим слоем не видны. Правка interval.c diff не даёт.
   Golden поэтому не подлежит регенерации «ради зелёного CI»: его движение
   всегда семантическое.

Байты допущенных исходников закреплены отдельно — ``_PINNED_BUILD_SOURCE_SHA256_V1``
в ``arb/pipeline.py``. Слои пересекаются, и это надо знать перед правкой:
reference-вектор сворачивает содержимое допущенных файлов транзитивно, поэтому
правка любого ``arb/evaluator/*.c|*.h`` требует репина в ДВУХ местах — в
``_PINNED_BUILD_SOURCE_SHA256_V1`` и в ``SOURCE_BOUND_PREIMAGE_SHA256_V1``
(координата ``wrapper_source``). Обновление только первого оставляет вектор
красным. Структурный golden при этом зелёный: он не видит байты чанков длиннее
64 Б, и именно поэтому его нельзя «перегенерировать ради зелёного CI».

Две координаты BUILD-наблюдения (``build_identity``, ``test_observation``)
намеренно не входят в reference-вектор: их байты законно зависят от кодировки
процессов сборки. Структурный golden покрывает только порядок и число их
чанков; байты процессов — вне этого слоя, их закрепляет golden в
``tests/test_build.py``. Перестановка stdout/stderr внутри
``build_process_bytes_v1`` этим модулем не ловится — это известная граница
слоя, не покрытие.
"""

from __future__ import annotations

import hashlib
import sys
import unittest
from dataclasses import fields as dataclass_fields
from pathlib import Path


PROOF = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(PROOF))

# Детерминированный вход берётся из уже существующего фикстур-закрытия
# ``test_pipeline``, а не переписывается заново: второй экземпляр синтетических
# архивов расходился бы с первым и вектор перестал бы что-либо значить.
import test_pipeline  # noqa: E402
from arb import pipeline  # noqa: E402
import region_proof_protocol as protocol  # noqa: E402


COMPARATOR_LABEL_PREFIX_V1 = "labcolors.proof-region.arb-comparator."

# Точные байты каждой source-bound координаты на детерминированном входе
# (синтетический source-закрытие из test_pipeline + допущенные локальные
# исходники). Ключ — ИМЯ координаты: перестановка двух координат местами
# меняет содержимое поля и краснит вектор.
SOURCE_BOUND_PREIMAGE_SHA256_V1 = {
    "engine_release": "0b9557439aac7b93eceec78a93d44c00c77a565110fcfe8f90c139bed03b46f1",
    "upstream_source": "7d0970948bbc52f91b1211ca985773eabb86007be0ce0d5cb02060e1979af29c",
    "arithmetic_input_set": "006a093a27215c5fec14454ac6ec82f008519a437b98001d95d47689d703694d",
    "wrapper_source": "957eb113f89a191ad0eade3cd2f9f3a9a1da021694921a670efa86949e680dc2",
    "evaluator_source": "9e4f470d56a954a270f636c6c7befca26a77549f345cdaaa83cf35e097952db2",
    "operation_allowlist": "bae12a908ba87c59c8126811739e85a472218f75c9325d47c2f72f2b7ddf2b79",
    "legal_file_set": "5d9b4658d60406d014ddcc7f6ce47966fb795826ce0440d6d0186e283bcc43e1",
    "exclusions": "3c9c249541dced3d1a35159244b4b6feebfa0d290587fdf0cd3c2c98f5941b41",
}

# Содержимое каждой координаты в разобранном виде. `file:<path>` вместо байтов
# допущенного файла — потому что содержимое файлов закреплено манифестом
# pipeline, а здесь закрепляется смысл, а не байты. Любое движение этого
# golden — семантическое.
COORDINATE_STRUCTURE_GOLDEN_V1 = """
engine_release label=labcolors.proof-region.arb-comparator.engine-release.v1 version=1 chunks=3
  [00] "FLINT release lock declaration"
  [01] bin
  [02] bin:32
upstream_source label=labcolors.proof-region.arb-comparator.upstream-source.v1 version=1 chunks=33
  [00] bin
  [01] bin:32
  [02] bin:4
  [03] bin:1
  [04] bin
  [05] bin:32
  [06] bin:32
  [07] bin:32
  [08] bin:8
  [09] bin:8
  [10] bin
  [11] bin:8
  [12] bin:32
  [13] bin:1
  [14] bin
  [15] bin:32
  [16] bin:32
  [17] bin:32
  [18] bin:8
  [19] bin:8
  [20] bin
  [21] bin:8
  [22] bin:32
  [23] bin:1
  [24] bin
  [25] bin:32
  [26] bin:32
  [27] bin:32
  [28] bin:8
  [29] bin:8
  [30] bin
  [31] bin:8
  [32] bin:32
arithmetic_input_set label=labcolors.proof-region.arb-comparator.arithmetic-input-set.v1 version=1 chunks=18
  [00] "exact admitted GMP MPFR FLINT source snapshots and pinned static-build boundary"
  [01] bin:4
  [02] bin:1
  [03] bin:32
  [04] bin:32
  [05] bin:32
  [06] bin:1
  [07] bin:32
  [08] bin:32
  [09] bin:32
  [10] bin:1
  [11] bin:32
  [12] bin:32
  [13] bin:32
  [14] "gcc@sha256:c74b2d34b775e6a1b14b13b1d41dc7233f62a18f7a6a4e139e0cf59eeab2e070"
  [15] "linux/amd64"
  [16] bin:32
  [17] bin:32
wrapper_source label=labcolors.proof-region.arb-comparator.wrapper-source.v1 version=1 chunks=13
  [00] bin:4
  [01] "proof/region/v1/arb/evaluator/formula.h"
  [02] bin:4
  [03] bin:8
  [04] file:proof/region/v1/arb/evaluator/formula.h
  [05] "proof/region/v1/arb/evaluator/interval.c"
  [06] bin:4
  [07] bin:8
  [08] file:proof/region/v1/arb/evaluator/interval.c
  [09] "proof/region/v1/arb/evaluator/interval.h"
  [10] bin:4
  [11] bin:8
  [12] file:proof/region/v1/arb/evaluator/interval.h
evaluator_source label=labcolors.proof-region.arb-comparator.evaluator-source.v1 version=1 chunks=33
  [00] bin:4
  [01] "generated/formula.generated.c"
  [02] bin:4
  [03] bin:8
  [04] file:generated/formula.generated.c
  [05] "proof/region/v1/arb/evaluator/hash.c"
  [06] bin:4
  [07] bin:8
  [08] file:proof/region/v1/arb/evaluator/hash.c
  [09] "proof/region/v1/arb/evaluator/hash.h"
  [10] bin:4
  [11] bin:8
  [12] file:proof/region/v1/arb/evaluator/hash.h
  [13] "proof/region/v1/arb/evaluator/main.c"
  [14] bin:4
  [15] bin:8
  [16] file:proof/region/v1/arb/evaluator/main.c
  [17] "proof/region/v1/arb/evaluator/region.c"
  [18] bin:4
  [19] bin:8
  [20] file:proof/region/v1/arb/evaluator/region.c
  [21] "proof/region/v1/arb/evaluator/region.h"
  [22] bin:4
  [23] bin:8
  [24] file:proof/region/v1/arb/evaluator/region.h
  [25] "proof/region/v1/arb/evaluator/wire.c"
  [26] bin:4
  [27] bin:8
  [28] file:proof/region/v1/arb/evaluator/wire.c
  [29] "proof/region/v1/arb/evaluator/wire.h"
  [30] bin:4
  [31] bin:8
  [32] file:proof/region/v1/arb/evaluator/wire.h
build_identity label=labcolors.proof-region.arb-comparator.build-identity.v2 version=2 chunks=14
  [00] file:proof/region/v1/arb/build.sh
  [01] bin:32
  [02] bin:32
  [03] bin:32
  [04] bin:32
  [05] "build-observation=diagnostic-unsealed-v1"
  [06] bin:4
  [07] bin
  [08] bin
  [09] bin:32
  [10] bin:32
  [11] bin:32
  [12] bin:8
  [13] bin:32
operation_allowlist label=labcolors.proof-region.arb-comparator.operation-allowlist.v1 version=1 chunks=22
  [00] "exact-real-ssa-operator-declarations"
  [01] bin:4
  [02] "operator lookup 2 real table_u8_exact_dyadic_at_ordinal"
  [03] "operator eq 2 bool exact_same_type_equality"
  [04] "operator select 3 same bool_true_second_else_third"
  [05] "operator add 2 real exact_x_plus_y"
  [06] "operator sub 2 real exact_x_minus_y"
  [07] "operator mul 2 real exact_x_times_y"
  [08] "operator div 2 real domain_y_ne_zero_x_div_y_else_domain_unproven"
  [09] "operator min 2 real exact_lesser_real"
  [10] "operator max 2 real exact_greater_real"
  [11] "operator root3 1 real domain_x_ge_zero_unique_y_ge_zero_y_cubed_eq_x_else_domain_unproven"
  [12] "operator sqrt 1 real domain_x_ge_zero_unique_y_ge_zero_y_squared_eq_x_else_domain_unproven"
  [13] "operator exp 1 real analytic_natural_exponential"
  [14] "operator log 1 real domain_x_gt_zero_analytic_natural_logarithm_else_domain_unproven"
  [15] "operator sin 1 real analytic_sine_radians"
  [16] "operator cos 1 real analytic_cosine_radians"
  [17] "operator abs 1 real exact_absolute_value"
  [18] "operator sign 1 real negative_minus_one_zero_zero_positive_one"
  [19] "operator pow_pos 2 real domain_x_gt_zero_exp_y_mul_log_x_else_domain_unproven"
  [20] "operator pow_nn 2 real if_x_eq_zero_and_y_gt_zero_zero_else_pow_pos"
  [21] "operator ratio0 2 real if_x_eq_zero_and_y_eq_zero_zero_else_domain_y_gt_zero_x_div_y"
test_observation label=labcolors.proof-region.arb-comparator.test-observation.v1 version=1 chunks=5
  [00] "kind:aggregate-outer-process-observation-no-per-test-records"
  [01] file:proof/region/v1/arb/build.sh
  [02] bin:4
  [03] bin
  [04] bin
legal_file_set label=labcolors.proof-region.arb-comparator.legal-file-set.v1 version=1 chunks=32
  [00] "ordered admitted legal-file set; no legal-compliance claim"
  [01] bin:4
  [02] bin:1
  [03] bin:32
  [04] bin:32
  [05] bin:32
  [06] bin:4
  [07] bin:51
  [08] "LICENSE"
  [09] bin:4
  [10] bin:8
  [11] bin:32
  [12] bin:1
  [13] bin:32
  [14] bin:32
  [15] bin:32
  [16] bin:4
  [17] bin:51
  [18] "LICENSE"
  [19] bin:4
  [20] bin:8
  [21] bin:32
  [22] bin:1
  [23] bin:32
  [24] bin:32
  [25] bin:32
  [26] bin:4
  [27] bin:51
  [28] "LICENSE"
  [29] bin:4
  [30] bin:8
  [31] bin:32
exclusions label=labcolors.proof-region.arb-comparator.exclusions.v1 version=1 chunks=12
  [00] "gap:host-and-docker-daemon-not-source-bound"
  [01] "gap:unsealed-diagnostic-build-observer"
  [02] "gap:libc-libm-libpthread-libgcc-and-build-utility-source"
  [03] "gap:no-per-test-result-records"
  [04] "gap:no-git-derivation-for-project-pinned-release-only-files"
  [05] "gap:no-origin-authority-reverification"
  [06] "unsealed-linux-x64-docker-host"
  [07] "build-observation=diagnostic-unsealed-v1"
  [08] bin:4
  [09] "ci/omitted"
  [10] bin:4
  [11] bin:57
""".strip()


class PreimageDecodeErrorV1(ValueError):
    """Разбор не восстановил ровно исходные байты преимиджа."""


def decode_comparator_preimage_v1(
    preimage: bytes,
) -> tuple[bytes, int, tuple[bytes, ...]]:
    """Независимый тотальный разбор преимиджа компаратора.

    Оракул написан от формата провода, а не вызовом кодировщика pipeline:
    закон именования и golden иначе доказывали бы сами себя. Разбор обязан
    потребить ровно все байты — иначе схема сдвинулась и утверждать по ней
    нельзя.
    """

    if type(preimage) is not bytes or not preimage:
        raise PreimageDecodeErrorV1("preimage must be nonempty bytes")
    separator = preimage.find(b"\0")
    if separator < 1:
        raise PreimageDecodeErrorV1("preimage has no domain separator")
    label = preimage[: separator + 1]
    header = separator + 1
    if len(preimage) < header + 5:
        raise PreimageDecodeErrorV1("preimage is truncated before its chunk count")
    version = preimage[header]
    count = int.from_bytes(preimage[header + 1 : header + 5], "big")
    offset = header + 5
    chunks: list[bytes] = []
    for _ in range(count):
        if len(preimage) < offset + 8:
            raise PreimageDecodeErrorV1("preimage is truncated inside a chunk length")
        length = int.from_bytes(preimage[offset : offset + 8], "big")
        offset += 8
        if len(preimage) < offset + length:
            raise PreimageDecodeErrorV1("preimage is truncated inside a chunk body")
        chunks.append(preimage[offset : offset + length])
        offset += length
    if offset != len(preimage):
        raise PreimageDecodeErrorV1("preimage has trailing bytes after its chunks")
    return label, version, tuple(chunks)


def _render_chunk_v1(chunk: bytes, admitted_by_content: dict[bytes, str]) -> str:
    """Рендер одного чанка без байтов допущенного исходника."""

    path = admitted_by_content.get(chunk)
    if path is not None:
        return f"file:{path}"
    printable = chunk and all(0x20 <= byte <= 0x7E and byte != 0x22 for byte in chunk)
    if printable and len(chunk) <= 128:
        return f'"{chunk.decode("ascii")}"'
    # Длину печатаем только для коротких чанков: они структурные (счётчики,
    # дайджесты, роли). Длинные — это содержимое, и его размер здесь не
    # закрепляется, иначе golden двигался бы от правки исходника.
    return f"bin:{len(chunk)}" if len(chunk) <= 64 else "bin"


def render_coordinate_structure_v1(
    preimages: pipeline.ArbComparatorPreimagesV1,
    admitted_by_content: dict[bytes, str],
) -> str:
    lines: list[str] = []
    for field in dataclass_fields(preimages):
        label, version, chunks = decode_comparator_preimage_v1(
            getattr(preimages, field.name)
        )
        lines.append(
            f"{field.name} label={label[:-1].decode('ascii')} "
            f"version={version} chunks={len(chunks)}"
        )
        lines.extend(
            f"  [{index:02d}] {_render_chunk_v1(chunk, admitted_by_content)}"
            for index, chunk in enumerate(chunks)
        )
    return "\n".join(lines)


def _derived_comparator_v1() -> pipeline.DiagnosticArbComparatorV1:
    binary = test_pipeline._static_elf(b"derived-comparator")
    result = pipeline.ControlledPipelineV1(
        build_backend=test_pipeline._BuildBackend((binary, binary)),
    ).build(test_pipeline._request())
    if type(result) is not pipeline.DiagnosticBuildObservationV1:
        # Не печатаем сам объект: наблюдение сборки несёт мегабайтные байты
        # процессов и преимиджей. Тип + sha256 от repr идентифицируют отказ.
        digest = hashlib.sha256(repr(result).encode("utf-8", "replace")).hexdigest()
        raise AssertionError(f"{type(result).__name__} repr-sha256:{digest}")
    return result.comparator


def _admitted_by_content_v1() -> dict[bytes, str]:
    files = test_pipeline._build_sources().files
    by_content = {item.contents: item.path for item in files}
    if len(by_content) != len(files):
        raise AssertionError("admitted build sources are not content-distinct")
    return by_content


class PreimageDecoderTests(unittest.TestCase):
    """Оракул обязан быть чувствителен сам по себе."""

    def test_decoder_recovers_exact_parts_of_a_hand_built_preimage(self) -> None:
        preimage = (
            b"labcolors.proof-region.arb-comparator.example.v1\0"
            + b"\x01"
            + (2).to_bytes(4, "big")
            + (5).to_bytes(8, "big")
            + b"alpha"
            + (0).to_bytes(8, "big")
        )

        label, version, chunks = decode_comparator_preimage_v1(preimage)

        self.assertEqual(label, b"labcolors.proof-region.arb-comparator.example.v1\0")
        self.assertEqual(version, 1)
        self.assertEqual(chunks, (b"alpha", b""))

    def test_decoder_rejects_truncation_trailing_bytes_and_a_missing_separator(
        self,
    ) -> None:
        valid = (
            b"labcolors.proof-region.arb-comparator.example.v1\0"
            + b"\x01"
            + (1).to_bytes(4, "big")
            + (5).to_bytes(8, "big")
            + b"alpha"
        )
        mutants = (
            valid[:-1],
            valid + b"\x00",
            valid.replace(b"\0", b"!"),
            b"",
            valid[: valid.index(b"\0") + 3],
        )

        for index, mutant in enumerate(mutants):
            with self.subTest(mutant=index):
                with self.assertRaises(PreimageDecodeErrorV1):
                    decode_comparator_preimage_v1(mutant)

    def test_decoder_reproduces_every_derived_coordinate_byte_for_byte(self) -> None:
        preimages = _derived_comparator_v1().preimages

        for field in dataclass_fields(preimages):
            with self.subTest(coordinate=field.name):
                preimage = getattr(preimages, field.name)
                label, version, chunks = decode_comparator_preimage_v1(preimage)
                replayed = (
                    label
                    + bytes((version,))
                    + len(chunks).to_bytes(4, "big")
                    + b"".join(
                        len(chunk).to_bytes(8, "big") + chunk for chunk in chunks
                    )
                )
                # Сравниваем хеши, а не байты: при расхождении assertEqual
                # печатал бы 144-КБ преимиджи целиком. Имя координаты — в
                # сообщении, чтобы отказ оставался адресным.
                self.assertEqual(
                    hashlib.sha256(replayed).hexdigest(),
                    hashlib.sha256(preimage).hexdigest(),
                    field.name,
                )


class CoordinateNamingLawTests(unittest.TestCase):
    def test_every_coordinate_carries_the_domain_separator_that_names_it(self) -> None:
        preimages = _derived_comparator_v1().preimages
        labels: list[bytes] = []

        for field in dataclass_fields(preimages):
            with self.subTest(coordinate=field.name):
                label, version, _chunks = decode_comparator_preimage_v1(
                    getattr(preimages, field.name)
                )
                labels.append(label)
                mnemonic = field.name.replace("_", "-")
                self.assertEqual(
                    label.decode("ascii"),
                    f"{COMPARATOR_LABEL_PREFIX_V1}{mnemonic}.v{version}\0",
                )

        self.assertEqual(len(set(labels)), len(labels))
        self.assertEqual(len(labels), 10)


class SourceBoundReferenceVectorTests(unittest.TestCase):
    def test_the_vector_covers_exactly_the_source_bound_coordinate_set(self) -> None:
        # Вектор выводится из протокола, а не переписывается рукой: новая
        # source-bound координата обязана получить свои байты, а не проехать
        # мимо закрепления.
        self.assertEqual(
            tuple(SOURCE_BOUND_PREIMAGE_SHA256_V1),
            protocol.source_bound_coordinates_v2(),
        )
        self.assertEqual(len(SOURCE_BOUND_PREIMAGE_SHA256_V1), 8)

    def test_source_bound_coordinates_keep_their_exact_reference_preimages(
        self,
    ) -> None:
        preimages = _derived_comparator_v1().preimages

        for name, expected in SOURCE_BOUND_PREIMAGE_SHA256_V1.items():
            with self.subTest(coordinate=name):
                self.assertEqual(
                    hashlib.sha256(getattr(preimages, name)).hexdigest(),
                    expected,
                )


class CoordinateStructureGoldenTests(unittest.TestCase):
    def test_coordinate_structure_matches_the_content_free_golden(self) -> None:
        comparator = _derived_comparator_v1()

        self.assertEqual(
            render_coordinate_structure_v1(
                comparator.preimages,
                _admitted_by_content_v1(),
            ),
            COORDINATE_STRUCTURE_GOLDEN_V1,
        )

    def test_the_golden_holds_no_admitted_source_bytes(self) -> None:
        # Доказывает разделение слоёв: правка допущенного файла не двигает
        # этот golden, поэтому любое его движение — семантическое, и
        # регенерация «ради зелёного CI» невозможна.
        admitted = _admitted_by_content_v1()
        rendered = render_coordinate_structure_v1(
            _derived_comparator_v1().preimages,
            admitted,
        ).encode("ascii")

        for contents, path in admitted.items():
            with self.subTest(path=path):
                self.assertEqual(
                    _render_chunk_v1(contents, admitted),
                    f"file:{path}",
                )
                self.assertNotIn(contents, rendered)


if __name__ == "__main__":
    unittest.main()
