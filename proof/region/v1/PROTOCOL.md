# Offline proof protocol V1

Этот reference задаёт канонические типы, wire-кодек, identities и admission
для `proof/region/v1`.

## Граница

Протокол переносит immutable job и заявленные результаты будущих Arb/MPFI
processes. `region_proof_protocol.py` определяет только structural codecs и
admission функций сравнения. Текущий `controller.py` безопасно читает и
повторно проверяет пять frozen protocol fixtures; он ещё не строит и не
запускает evaluator, не разрешает comparator manifest и не создаёт provenance
receipt. Ни один текущий модуль не вычисляет цвет или interval enclosure и не
создаёт semantic proof type.

`V5b2c-0` определяет protocol/admission, но сам не является математическим
proof. В c0 нет `DualProofReceiptV1`: structural agreement кодируется
только как `DualComparisonCandidateV1`. C0 не создаёт полный family image,
`SemanticFamilyReleaseIdV2`, `FamilyArtifactReceiptIdV2` или
`FamilyImageCertificateV2`.

Синтетический успешный transcript или comparison candidate не является
evidence. В тесте протокола такое значение явно остаётся test-only и не
хранится или не публикуется как proof artifact.

Протокол не входит в Cargo workspace, Core, WASM, FFI, bindings или packages.
Раздельные evaluator implementations и их допустимый общий dependency overlap
появятся в следующих срезах; c1a ещё не подтверждает их происхождение или
diversity.

## Wire и identity

Все целые беззнаковые и записаны big-endian как `u8`, `u32be` или `u64be`.
`digest` — ровно 32 ненулевых bytes SHA-256. `blob` равен
`u64be(length) || bytes`. Enum занимает один `u8` и принимает только
перечисленные значения. Padding, alignment, reserved fields и trailing bytes
отсутствуют.

До allocation и цикла по records parser проверяет арифметику длины без
переполнения, остаток input, точный или минимальный wire-размер всех
объявленных элементов и только определённые typed semantic limits.
Неизвестный tag, truncation, oversized length/count, непрочитанный byte или
non-zero decision padding являются ошибкой admission. Decoder потребляет весь
artifact, а повторный encode обязан вернуть byte-identical bytes.

Каждый top-level artifact начинается с восьмибайтного magic. Общего header или
второго envelope поверх него нет. Его content identity равен:

`SHA256(identity_label || NUL || u64be(encoded_length) || encoded_bytes)`.

| Тип | Magic | `identity_label` |
| --- | --- | --- |
| `ReducedDomainManifestV1` | `LCDOM1\0\0` | `labcolors.proof-region.domain.v1` |
| `ProofPolicyV1` | `LCPOL1\0\0` | `labcolors.proof-region.policy.v1` |
| `ProofJobV1` | `LCJOB1\0\0` | `labcolors.proof-region.job.v1` |
| `ComparatorManifestV1` | `LCMAN1\0\0` | `labcolors.proof-region.comparator-manifest.v1` |
| `DecisionTranscriptV1` | `LCTRN1\0\0` | `labcolors.proof-region.transcript.v1` |
| `RunClaimV1` | `LCRUN1\0\0` | `labcolors.proof-region.run-claim.v1` |
| `EvaluatorProvenanceClaimV1` | `LCPRV1\0\0` | `labcolors.proof-region.evaluator-provenance-claim.v1` |
| `DualComparisonClaimV1` | `LCCMP1\0\0` | `labcolors.proof-region.dual-comparison.v1` |

## `ContextualRegionDefinitionV1`

Definition не получает protocol magic. Это точный V5b2b canonical preimage:
последовательность полей `u64be(field_length) || field_bytes`. Grammar содержит
`22 + 4 × knot_count` полей; fixture-specific count не является grammar.
Поле 21 содержит `knot_count: u64be`; count ненулевой, но не имеет ad-hoc cap.
До чтения knots parser требует, чтобы остаток input был ровно
`knot_count × 4 × (8-byte length + 8-byte payload)`. Так wire bytes, а не
произвольный protocol limit, ограничивают count до цикла по records.

Admission строго парсит typed V5b2b definition, проверяет его domain-инварианты,
повторно кодирует и требует byte-identical preimage. Заявленный
`FamilyDefinitionDigestV2` равен `SHA256(definition_preimage)`.

Formula digest внутри definition должен совпасть с приложенным immutable strict
US-ASCII SSA blob. Для V1 `exact_spec_bytes` имеет ровно 24 434 bytes
(`FORMULA_SPEC_BYTES_V1 = 24_434`):

`SHA256("labcolors.nominal-exact-real-lift.ascii-ssa.v1\0" ||
u64be(spec_length) || exact_spec_bytes)`.

Trim, изменение line endings, normalisation или подмена formula-spec bytes
запрещены.

## `ReducedDomainManifestV1`

Wire после `LCDOM1\0\0`, по порядку:

1. ordinal release `u8`; V1 допускает только `1` — sRGB8
   `ordinal = (r << 16) | (g << 8) | b`;
2. declared point count `u64be`;
3. range count `u64be`;
4. столько records `start_inclusive: u32be || end_exclusive: u32be`.

Manifest непуст. Каждый range непуст и лежит внутри `[0, 2^24)`. Ranges строго
возрастают; overlap, duplicate, reorder и adjacency запрещены. Для соседей
обязательно `previous.end_exclusive < next.start_inclusive`: соприкасающиеся
ranges должны быть слиты. Промежутки разрешены. Сумма длин ranges точно равна
declared point count. Поэтому до чтения records parser выводит точную верхнюю
границу `range_count <= min(point_count, 2^24 - point_count + 1)`: каждый range
занимает хотя бы одну точку, а соседние ranges требуют хотя бы один пропуск.
Каждый record проверяется до чтения следующего; невозможный первый range не
может принудить parser материализовать объявленный хвост.

Точки перечисляются по ranges, затем по возрастающему ordinal внутри range.
Этот порядок единственный для policy accounting, decisions и witnesses.

## `ProofPolicyV1`

Wire после `LCPOL1\0\0`, по порядку:

1. equality release `u8`; V1 допускает только
   `1 = ExactZeroSignalTraceV1`;
2. comparator count `u8`, всегда `2`;
3. `ComparatorBudgetV1` сначала для `1 = Arb`, затем для `2 = MPFI`.

Один `ComparatorBudgetV1` содержит comparator kind `u8`, rung count `u32be`,
столько binary precision rungs `u32be`, per-point work limit `u64be` и global
pregrant limit `u64be`.

Ladder непуст, положителен и строго возрастает. Один work unit — одна evaluation
канонической region branch для одного ordinal на одном rung. Global grants
выдаются ordinal-prefix до worker execution и не переносятся между points.
Нулевой limit допустим и даёт `ResourceLimitReached`, а не parse error. Wall
clock, RAM и scheduling не входят в policy или verdict.

## `ProofJobV1`

Wire после `LCJOB1\0\0`, по порядку:

1. definition digest;
2. definition preimage `blob`;
3. formula release digest;
4. formula spec `blob`;
5. domain identity;
6. encoded `ReducedDomainManifestV1` как `blob`;
7. policy identity;
8. encoded `ProofPolicyV1` как `blob`.

Admission проверяет definition и formula identities, canonical re-encode и
identity каждого вложенного artifact. Formula release обязан совпасть с полем
definition. Job задаёт единственный канонический input contract будущих
вычислителей; только controlled-executor slice сможет доказать отсутствие
ambient inputs. Альтернативный JSON/TOML definition запрещён протоколом.

## `ComparatorManifestV1`

Wire после `LCMAN1\0\0` содержит comparator kind `u8`
(`1 = Arb`, `2 = MPFI`), затем десять digest coordinates в фиксированном
порядке:

1. engine release;
2. upstream source;
3. arithmetic closure;
4. wrapper source;
5. evaluator source;
6. build identity, включая compiler, target и exact flags;
7. operation allowlist;
8. test receipt;
9. license closure;
10. exclusions.

Результат wire parse — только raw `ComparatorManifestV1`: его ненулевые
coordinates являются заявленными content addresses, а не доказанным
source binding. `ContentResolvedComparatorManifestV1` создаётся только
после того, как переданный вызывающим `resolve_content_address` для каждой из десяти
coordinates вернул exact `bytes` или `Iterable[bytes]`. Сам protocol повторяет
SHA-256 по этим bytes/chunks и сравнивает результат с coordinate. Boolean,
membership-only ответ или чужой готовый digest не могут создать refined type.
Отсутствующий referent или chunk не типа `bytes` дают `invalid_manifest`,
а несовпавший replay digest — `digest_mismatch`.

Content resolution проверяет только совпадение заявленного coordinate с
переданными bytes. Оно не устанавливает доверие к resolver, причинность,
корректность evaluator, build или test claim и не создаёт semantic
source-bound/proven type.

V1 не доказывает independence. Как anti-vacuum declared-diversity gate,
`compare_dual_transcripts` требует canonical kinds `ComparatorKindV1.ARB` →
`ComparatorKindV1.MPFI` и попарно различные `engine_release`,
`upstream_source`, `wrapper_source`, `evaluator_source` и
`RunClaimV1.binary_identity`. Это лишь различие заявленных coordinates: оно не
доказывает разное происхождение или реализацию. Допустимый общий GMP/MPFR
overlap и обязательные distinct edges появятся в typed replay evidence.

## `DecisionTranscriptV1`

Wire после `LCTRN1\0\0`, по порядку:

1. job identity;
2. domain identity;
3. comparator manifest identity;
4. point count `u64be`;
5. decision payload `blob`;
6. counters `inside`, `outside`, `boundary_unproven`,
   `resource_limit_reached` как четыре `u64be`;
7. exact-equality count `u64be`;
8. work-accounting digest;
9. witness count `u64be`;
10. столько `WitnessRecordV1`.

На каждую domain point приходится ровно две MSB-first bits:

| Bits | Outcome |
| --- | --- |
| `00` | `Inside` |
| `01` | `Outside` |
| `10` | `BoundaryUnproven` |
| `11` | `ResourceLimitReached` |

Длина payload равна `ceil(point_count × 2 / 8)`. Неиспользованные младшие bits
последнего byte равны нулю. Point count равен domain point count; outcome с
индексом `i` относится к `i`-й точке canonical domain order. Counters совпадают
с payload и в сумме дают point count.

При parse packed payload считается по MSB-first outcomes без создания
промежуточной коллекции decisions. До чтения первого witness parser
повторно выводит четыре counters из payload, проверяет их сумму,
`exact_equality_count <= inside`, точный witness count
`boundary_unproven + resource_limit_reached + exact_equality_count` и точный
остаток witness body:

`37 × (boundary_unproven + exact_equality_count) +
22 × resource_limit_reached` bytes.

Размеры 37 и 22 следуют из wire records ниже; truncation или trailing
bytes отклоняются до цикла по records. `DecisionTranscriptV1.from_decisions`
за один проход пакует iterable outcomes и считает counters. При binding
выравнивание ordered witnesses с domain ranges и packed outcomes идёт одним
проходом по ranges/witnesses и не материализует полный domain или decisions.

`WitnessStoreV1` сохраняет canonical witness body как span внутри исходных
immutable bytes. Parse одним raw-проходом проверяет tag, длину, ordinal order,
digests, resource accounting и кеширует counts, не создавая tuple или объект на
каждый record. Alignment, identity и byte comparison также идут по raw bytes;
typed witnesses создаются лениво только явным `iter_witnesses()`. Generated
path потребляет witness iterable один раз. Поэтому допустимый full-domain
unresolved transcript удерживает неизбежные wire bytes, но не 2^24 Python
объектов и не повторно кодирует каждый record для identity.

Witness records имеют фиксированный wire:

- `ExactZeroSignalTraceV1`:
  `kind=1 || ordinal:u32be || trace_digest`;
- `BoundaryUnprovenWitnessV1`:
  `kind=2 || ordinal:u32be || enclosure_digest`;
- `ResourceLimitWitnessV1`:
  `kind=3 || ordinal:u32be || scope:u8 || granted:u64be || consumed:u64be`,
  где `scope 1 = per-point`, `2 = global` и `consumed = granted`.

Witness ordinals строго возрастают, уникальны и принадлежат manifest. Kind
обязан совпадать с outcome этой точки; каждый unresolved outcome имеет ровно
один соответствующий witness. Exact-zero witness допустим только для `Inside`,
а число таких records равно exact-equality count. Missing/extra/foreign witness
не может быть исправлен самим битом `Inside`.

`trace_digest` и `enclosure_digest` в c0 — только ненулевые content
coordinates, а не доказательство replay или enclosure. Будущий semantic
verifier receipt обязан разрешить и replay эти records, проверить exact
equality/enclosure math и связать результат с job, comparator, run и transcript.
Controller этого не делает. Любая недоказанная transcendental equality
остаётся `BoundaryUnproven`: epsilon или midpoint не превращают её в
`Inside`.

`DecisionTranscriptV1` в c0 остаётся structural claim. Нулевые unresolved
counters не превращают его в semantic resolved/proven type.

## `RunClaimV1`

Wire после `LCRUN1\0\0` содержит шесть digest coordinates:

1. job identity;
2. comparator manifest identity;
3. exact binary identity;
4. exact invocation identity;
5. platform identity;
6. transcript identity.

Wire parse и `for_transcript` создают только structural run claim из заявленных
coordinates. `for_transcript` проверяет лишь bindings job/comparator/transcript;
binary, invocation и platform получает от вызывающего и не наблюдает. Причинную
цепь сможет установить только будущий controlled rebuild/replay.

## `EvaluatorProvenanceClaimV1`

Wire после `LCPRV1\0\0` содержит три unresolved digest declarations:

1. provenance policy identity — versioned правила и trust boundary будущего replay;
2. `RunClaimV1` identity — subject, к которому относится заявление;
3. replay evidence identity — predicate с будущей source/build/run цепью.

Этот тип аналогичен структурному statement, а не attestation о выполненном
build. `parse` проверяет только canonical wire и ненулевые coordinates. В c1a
нет `SourceBoundEvaluatorReceiptV1`, public admission, resolver или флага
успешного replay. Sealed receipt появится только из реально наблюдаемого
rebuild/run и будет иметь отдельную domain-separated identity.

Назначение трёх внутренних coordinates только вдохновлено разделением ролей в
[in-toto Statement V1.2.0](https://github.com/in-toto/attestation/blob/v1.2.0/spec/v1/statement.md)
и definition/run model в
[SLSA Build Provenance V1.2](https://slsa.dev/spec/v1.2/build-provenance).
Wire остаётся внутренним бинарным протоколом Lab Colors; c1a не заявляет
in-toto Statement/ResourceDescriptor/predicate schema, envelope/signature,
SLSA level или соответствие SLSA builder contract.

## `DualComparisonClaimV1` и `DualComparisonCandidateV1`

Raw wire после `LCCMP1\0\0` содержит четыре digests, затем
`domain_point_count: u64be`, затем семь digests:

1. job identity;
2. definition digest;
3. domain identity;
4. policy identity;
5. domain point count;
6. Arb и MPFI comparator identities;
7. Arb и MPFI run claim identities;
8. Arb и MPFI transcript identities;
9. decision digest.

Canonical decision digest равен:

`SHA256("labcolors.proof-region.resolved-decisions.v1\0" || domain_identity ||
u64be(decision_payload_length) || decision_payload)`.

`DualComparisonClaimV1.parse` проверяет только canonical wire shape и возвращает
raw claim. Он никогда не возвращает admitted candidate. Непубличный constructor
`DualComparisonCandidateV1` вызывается только `compare_dual_transcripts` после
всех проверок ниже; произвольные корректно оформленные digests не могут создать
refined type.

Candidate строится в canonical order Arb → MPFI из двух
`ContentResolvedComparatorManifestV1`, согласованных `RunClaimV1` и
structurally admitted transcripts. Все bindings ведут к одному job, definition,
domain и policy; `domain_point_count` равен count связанного manifest.
Unresolved counters равны нулю, decision payloads совпадают побайтно,
а canonical raw equality witness bodies совпадают побайтно.

`BoundaryUnproven`, `ResourceLimitReached`, disagreement, foreign binding,
shared required-distinct coordinate или structurally missing equality witness возвращают typed
failure и не создают candidate. Успешный candidate фиксирует только
структурное согласие над exact bound domain manifest; он не является proof
receipt и не доказывает correctness ни одного evaluator.

Математический proof требует будущих semantic verifier receipts для обоих
evaluator paths и независимой проверки их bindings/replay. Family mint
дополнительно разрешает `domain_identity` и допускает отдельно exact full
manifest: единственный range `[0, 2^24)` и point count `2^24`. Совпадение
только point count или reduced-domain candidate этот gate не проходят.

## Ошибки admission

`ProtocolReasonV1` — закрытая сумма:

`bad_magic`, `truncated`, `trailing_bytes`, `length_out_of_bounds`,
`count_mismatch`, `unknown_release`, `invalid_definition`, `digest_mismatch`,
`empty_domain`, `noncanonical_order`, `overlapping_range`, `adjacent_range`,
`invalid_range`, `invalid_policy`, `invalid_manifest`, `invalid_digest`,
`invalid_transcript`, `missing_equality_witness`, `foreign_binding`,
`shared_diversity_coordinate`,
`unresolved_transcript`, `disagreement`.

`invalid_digest` означает coordinate неправильного типа, длины или нулевое
значение; `digest_mismatch` означает несовпадение корректно представленного
coordinate с replay или связанной identity.

Ошибка содержит artifact, byte offset, reason и detail, но не создаёт fallback
value. Raw parse, canonical admission, content resolution, transcript/run binding
и dual comparison являются разными structural переходами типов; ни один из
них не является semantic proof admission.
