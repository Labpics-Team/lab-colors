# Offline proof protocol V1

Этот reference задаёт канонические типы, wire-кодек, identities и admission
для `proof/region/v1`.

## Граница

Протокол переносит immutable job и structural claims результатов evaluator
processes. `region_proof_protocol.py` определяет только codecs и admission
функций сравнения, а `controller.py` повторно проверяет committed frozen
protocol fixtures и не является evaluator runner. `arb/evaluator` вычисляет
Arb-enclosures и выпускает связанные transcript bytes;
`SourceBoundArbControllerV1` заново собирает evaluator, запускает его и создаёт
только provenance receipt. Ни один из этих путей не выполняет независимый
semantic replay и не создаёт mathematical proof type. MPFI source lock,
archive admission и sealed source input (не evaluator replay) уже представлены.
`mpfi/evaluator` содержит отдельный source-owned M1.5 build/run path, а
`mpfi/receipt.py` — controller-only source-bound BUILD→RUN receipt boundary.
Semantic verifier для MPFI в текущем release всё ещё отсутствует.

Structural protocol/admission сам не является математическим proof.
`DualComparisonCandidateV1` кодирует только structural agreement и не создаёт
`DualProofReceiptV1`, полный family image,
`SemanticFamilyReleaseIdV2`, `FamilyArtifactReceiptIdV2` или
`FamilyImageCertificateV2`.

Синтетический успешный transcript или comparison candidate не является
evidence. В тесте протокола такое значение явно остаётся test-only и не
хранится или не публикуется как proof artifact.

Протокол не входит в Cargo workspace, Core, WASM, FFI, bindings или packages.
`SourceBoundEvaluatorReceiptV1` и `MpfiSourceBoundEvaluatorReceiptV1`
подтверждают причинную цепь только в пределах своих controller-owned
provenance boundaries. MPFI receipt не заявляет cross-path dependency overlap,
diversity или semantic correctness; structural coordinates и локальная сборка
этого не восполняют.

## Бинарный формат и идентичность

Для wire-artifact-ов из `region_proof_protocol.py` все целые беззнаковые и
записаны big-endian как `u8`, `u32be` или `u64be`; `digest` — ровно 32
ненулевых bytes SHA-256, а `blob` равен `u64be(length) || bytes`.
`SourceReleaseLockV1` и связанные provenance-artifact-ы имеют отдельный codec
в `provenance.py`: его `blob` равен `u32be(length) || bytes`; grammar также
содержит свои `u16` и 20-byte OpenPGP/SHA-1 coordinates. Enum занимает один
`u8` и принимает только перечисленные значения. Padding, alignment, reserved
fields и trailing bytes отсутствуют.

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
| `ComparatorManifestV2` | `LCMAN2\0\0` | `labcolors.proof-region.comparator-manifest.v2` |
| `DecisionTranscriptV1` | `LCTRN1\0\0` | `labcolors.proof-region.transcript.v1` |
| `RunClaimV1` | `LCRUN1\0\0` | `labcolors.proof-region.run-claim.v1` |
| `EvaluatorProvenanceClaimV1` | `LCPRV1\0\0` | `labcolors.proof-region.evaluator-provenance-claim.v1` |
| `DualComparisonClaimV1` | `LCCMP1\0\0` | `labcolors.proof-region.dual-comparison.v1` |

Версия принадлежит отдельному artifact type. Composite V1 wire связывает
identity независимо версионированного comparator manifest как opaque digest.

## `ContextualRegionDefinitionV1`

Definition не получает protocol magic. Это точный canonical preimage:
последовательность полей `u64be(field_length) || field_bytes`. Grammar содержит
`22 + 4 × knot_count` полей; fixture-specific count не является grammar.
Поле 21 содержит `knot_count: u64be`; count ненулевой, но не имеет ad-hoc cap.
До чтения knots parser требует, чтобы остаток input был ровно
`knot_count × 4 × (8-byte length + 8-byte payload)`. Так wire bytes, а не
произвольный protocol limit, ограничивают count до цикла по records.

Admission строго парсит typed contextual definition, проверяет его
domain-инварианты, повторно кодирует и требует byte-identical preimage. Заявленный
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
definition. Job задаёт единственный канонический input contract evaluator-ов.
Arb controller связывает его с наблюдёнными BUILD/RUN внутри объявленной ниже
границы доверия; receipt не заявляет отсутствие ambient inputs за пределами этой
границы. Альтернативный JSON/TOML definition запрещён протоколом.

### MPFI runtime profile V1

Wire grammar сама не превращается в неограниченный allocator. Прямой M1.5
executable принимает только профиль `LC-MPFI-RUNTIME-V1`: stdin job не более
16 MiB, не более 4096 bits на precision rung, не более 32 rung-ов, не более
1024 contextual knots и не более 16 MiB transcript output. Это operational
admission profile, а не математический предел definition/domain: лимиты job,
precision, rung-ов и knots возвращают typed `resource_limit` до MPFI
allocation, а переполнение transcript — typed `output_limit`. MPFI receipt
связывает тот же профиль с immutable executor limits и включает его в
source-bound BUILD/RUN evidence; прямой бинарь не является самостоятельным
public evaluator API.

В коде один exact tuple `MpfiRuntimeProfileV1` владеет этими пятью
координатами. `MpfiRuntimeBindingV1` связывает его с одним immutable
`ExecutionLimitsV1`: `max_stdin_bytes` обязан равняться `max_job_bytes`, а
`max_stdout_bytes` — `max_output_bytes`; остальные executor limits входят в
ту же binding identity явно. Поэтому контроллер не может заменить память,
время или stderr-лимит и сохранить тот же runtime-профиль. Ни executor, ни
общий protocol leaf не импортируют MPFI: это lane-specific contract,
включённый в MPFI source-bound receipt boundary.

## Фиксация источников и наблюдения целостности

`SourceReleaseLockV1` фиксирует bytes и структурный состав архива. Поле
`.integrity` содержит один `SourceIntegrityPolicyV1`; это точная граница
доступного evidence, а не безусловное заявление о publisher origin. Поле
`legal_files` — только точный project-pinned набор находящихся в архиве legal
files; оно не заявляет полноту legal-набора или compliance распространяемого
бинарника. Несовпадение этого набора имеет отдельную причину
`legal_files_mismatch`.

Для GMP и MPFR locked detached signature, key packets и исторический
`VALIDSIG` связываются только в
`HistoricalPathRecheckedSignatureDiagnosticV1`. Digest и version запущенного
`gpgv` остаются диагностикой. Запуск принадлежит переданному клиентом
`DiagnosticProcessRunnerV1`: Core ограничивает и парсит возвращённые bytes, но
не выдаёт runner за sandbox, containment или provenance authority. Встроенного
`Popen` fallback нет. Этот тип не устанавливает текущего publisher,
текущий статус или отзыв ключа, происхождение полученных bytes и exact sealed
execution verifier. Такой diagnostic сам по себе не создаёт и не заменяет
`SourceBoundEvaluatorReceiptV1` и не усиливает его до publisher claim.

Для FLINT `GitContentRelationPolicyV1` фиксирует commit, tree, исключённые
paths и отдельные `project_pinned_release_only_files`. `run_git_tree` принимает
такой же client-owned diagnostic runner, после чего Core независимо
пересчитывает commit, commit-to-tree edge, recursive tree и каждый
blob. Поэтому admission создаёт один `RecomputedGitContentRelationV1`: paths
архива должны быть точным дизъюнктным объединением общих Git files и
project-pinned release-only files, а исключённые paths обязаны отсутствовать.
Git executable/version, repository URL и tag являются диагностикой или
координатами поиска и не входят в authority этой relation. Relation доказывает
совпадение content graph, но не publisher или канал получения архива.

Для MPFI 1.5.4 не подтверждены detached signature, опубликованный upstream
checksum или равенство release-архива Git tag/tree. Поэтому
`ProjectPinnedArchiveDigestPolicyV1` намеренно не содержит внешнего payload:
Lab Colors фиксирует exact HTTPS URL, длину и SHA-256 полученных archive bytes,
но не приписывает этот digest издателю и не заявляет publisher authentication.
`MpfiSourceLockV1` использует те же единичные GMP/MPFR source declarations, что
и Arb, однако имеет отдельную aggregate identity и отдельный typed admission.

## Диагностическая граница исполнения

`proof/region/v1/executor.py` — общий для enclosure engines leaf без импорта
Arb/MPFI, formula или comparator semantics. Он же единолично кодирует
`execution-invocation.v1` и `execution-platform.v1`; engine pipeline не может
вводить параллельную identity того же процесса. Sandbox release
`labcolors.proof-region.executor.linux-x86_64.v1` намеренно не сохраняет
старую Arb-domain identity: это hard cut, а не compatibility alias.

`ControlledExecutorV1` — единственный владелец one-shot capability: новый,
неуспешный, перекрывающийся probe или замена backend отзывают ранее выданный
объект до RUN. Capability выпускается контроллером для одного probe-поколения
и одного process id; fork не дублирует право запуска. Backend сообщает только
наблюдённые свойства хоста, получает guard текущего probe и не может продлить
жизнь capability повторно используемым report-объектом.

`ExecutionRequestV1`, его limits и `SupportedV1` являются структурно
неизменяемыми значениями. Публичные execution identity functions воспроизводят
admission из точных координат и возвращают
`bytes | ExecutionIdentityRejectedV1`: foreign или даже намеренно forged
malformed value становится versioned typed rejection, а не новой identity и не
exception-channel. `ControlledExecutorV1` отвергает такой request до probe и
backend run как `ObserverFailureV1(REQUEST_NOT_ADMITTED)`.

Linux backend допускается лишь в отдельном helper process. Helper находится в
прямом дочернем cgroup объявленного parent, а весь parent subtree имеет
`pids.max = 2` и перед probe содержит ровно observer. Эти два task slots имеют
не эвристический смысл: один занимает observer, второй — либо новый thread,
либо единственный controlled child. Kernel pids controller атомарно разрешает
только один из вариантов; поэтому check→fork race не маскируется повторным
опросом `/proc`. Execution child дополнительно получает собственный
`pids.max = 1`, memory limit и `cgroup.kill`; фактические limits читаются назад
до запуска. Отсутствие этой структуры возвращает typed unsupported/setup
outcome. Этот runtime остаётся diagnostic observation и не создаёт receipt.
Самостоятельный `ControlledExecutorV1` по-прежнему остаётся только такой
observation. Право на Arb receipt получает не executor, а отдельный one-shot
`SourceBoundArbControllerV1`, который владеет всей цепью BUILD → RUN и не
принимает backend, capability либо diagnostic observation от вызывающего.

## Общая граница BUILD

Source replay имеет две намеренно разные стадии. Сначала provenance канонически
перепарсивает source lock, bounded-decompresses и сканирует archive, чтобы
сверить lock, manifest, tree и compressed bytes. Эта metadata replay не создаёт
отдельные file-byte buffers. Затем
`provenance.replay_materialize_admitted_source_v1` из одного такого replay
создаёт token-closed снимок lock, archive и exact relative regular files.
Aggregate Arb/MPFI admission владеет только свежими replayed archives; runtime
получает все три file-byte materializations только через один
`replay_admitted_source_closure_v1`. Общий leaf не вводит USTAR namespace,
recipe или engine semantics. Lane выбирает layout и связывает собственный
aggregate source capability. `proof/region/v1/build/input.py` принимает уже
нормализованные lane entries, кодирует один канонический USTAR и владеет точными input bytes.
`SealedInputV1` структурно неизменяем, связывает целостность байтов с opaque
caller digest и не утверждает recipe либо engine semantics. Resource bounds
передаёт lane: общий encoder не вводит собственный fixture-specific cap.

`mpfi/input.py` строит `SealedInputV1` только из одного owned replay snapshot
пары `MpfiSourceLockV1` и `AdmittedMpfiSourcesV1`. Тот же снимок даёт exact
regular files, aggregate identity и versioned MPFI-only namespace
`sources/<role>/<relative>` и связывает свежую aggregate source capability с
exact USTAR bytes. Роль, а не archive root, разделяет три source trees: lock
не требует уникальности root. Целостность `SealedInputV1` сама по себе не
доказывает принадлежность MPFI closure; это отдельно перепроверяет MPFI
source-input binding. Caller передаёт canonical `CanonicalInputLimitsV1`: lane сверяет
declared exact file count и payload closure до повторной materialization archive
bytes, а общий encoder сверяет все final USTAR bounds после materialization.
Limits — operational boundary, не
координата MPFI source-input binding и не build policy. Для неверного public
capability boundary возвращается `MpfiSourceInputErrorV1`; failure exact archive
replay остаётся `ProvenanceErrorV1`, а limits/USTAR rejection — `InputErrorV1`.
Эта ступень не вводит recipe, Docker policy, BUILD/RUN authority, executable,
comparator, receipt или semantic verifier.

`mpfi/build.py` — следующая отдельная BUILD-граница: она повторно принимает
только pinned workspace files, exact generated formula bytes и MPFI source
snapshot, затем строит один USTAR bundle. `MPFI_BUILD_TRANSPORT_POLICY_V1`
фиксирует отдельный linux/amd64 Clang-19 image manifest, bootstrap и bounded
tmpfs; sealed input identity связывает их с source/build bytes. Это ещё не
receipt: до source-bound controller и реального disposable BUILD→RUN здесь
нет provenance claim.

`proof/region/v1/build/transport.py` владеет immutable Docker policy,
одноразовым probe→build lease, bounded stdin/stdout observation, cleanup и
двумя свежими попытками. Доказательные координаты разделены по причинам:

1. transport policy identity связывает все поля точной policy;
2. native command contract identity связывает один типизированный grammar для
   probe, build и cleanup и один immutable child-launch context
   (environment, cwd, umask, stdio topology, FD и session behavior); фактический argv и
   Popen kwargs строятся только этими значениями;
3. daemon observation identity связывает только два raw probe stdout;
4. Docker capability identity связывает policy, command contract и exact CLI
   path, daemon observation и наблюдённые host uid/gid.

`BuildSessionV1` и каждый `DockerBuildRequestV1` сохраняют только ту же
capability, те же input bytes и output cap. Request не содержит host path,
CID file или имя контейнера: native adapter сам создаёт свежий приватный CID
path. Native cleanup поддерживается только в fresh one-job VM workflow Arb:
другой субъект с тем же effective UID либо Docker-daemon authority там не
сосуществует. Права `0700` закрывают лишь cross-UID pathname access и не
аутентифицируют same-UID writer. В этой объявленной operational boundary для
cleanup допускается только полный ID, который Docker записал в CID path; перед
`rm --force <id>` adapter сверяет, что `docker container inspect` вернул тот же
ID. Имя контейнера и fallback-координата в cleanup не участвуют. Вне этой
границы CID path не является доказательством ownership. Чужая либо не
полученная текущим probe capability отвергается до process spawn; ambient
path/user повторно не считываются. Разрешение принадлежит создавшему process:
fork и конкурентное повторное использование отвергаются до блокировки. После
возврата Popen handle `BaseException` до повторного выброса исходного
interruption запускает детерминированные попытки остановить и reap CLI, закрыть
streams и очистить допущенный container. Во время самого Popen construction
handle может ещё отсутствовать: тогда возможна только best-effort попытка CID
cleanup, без ложного заявления о reap CLI. `TwoBuildObservationV1` хранит обе успешные попытки и только
классифицирует их байты как identical или different, не называя пару
универсальным доказательством воспроизводимости. При отказе после создания
валидной session сохраняется весь уже завершённый causal prefix. Context-free
contract violation, обнаруженный до создания session (например, невалидная
session или сбой `TemporaryDirectory`), может вернуть `BuildRejectedV1` без
`session` и `completed_processes`.
Transport не знает formula, ELF, comparator или
source provenance: engine lane отдельно перепроверяет свой engine-owned input binding перед
каждым process и передаёт output admission. MPFI sealed source input сам по себе
не является MPFI build policy; `mpfi/build.sh` объявляет source-owned recipe,
а `mpfi/receipt.py` связывает его с BUILD/RUN observation. Production admission
всё ещё требует exact native BUILD→RUN gate на том же source head. Recipe не
заимствует Arb semantics.

## Воспроизведение Arb, связанное с источником

`PipelineRequestV1` до операции отдельно перепроверяет и владеет metadata-only
source closure: это ранняя integrity boundary для public input, не shared cache
операции. Затем `SourceBoundArbControllerV1` получает один detached operation
snapshot: канонический source lock, owned replayed archives и единственные для
этой операции file-byte materializations, заново допущенные копии build files,
job и limits. Он передаёт этот же private snapshot в `ControlledPipelineV1`;
самостоятельный BUILD создаёт snapshot сам до probe/spawn. Внутренний transport
recheck сверяет только owned snapshot, а public verifier независимо строит
новый snapshot из request, сохранённого внутри evidence, а не из исходного
объекта вызывающего. До replay он фиксирует structural projection всех
evidence coordinates и сверяет каждый используемый protocol identity cache с
независимо восстановленным canonical wire. Он принимает результат только если
та же projection на входе, после source replay и после edge replay совпадает.
Projection сверяет retained bytes, manifests и protocol wire, но не открывает
вторую source materialization; она доказывает стабильность значения в пределах
одного вызова, а не неизменность объекта после возврата. Один
immutable bundle object дважды передаётся через bounded stdin; каждый свежий контейнер до
распаковки сверяет exact length и SHA-256, распаковывает только в private
bounded tmpfs, а executable возвращает через stdout. Semantic host bind mounts,
host output path и повторное открытие результата отсутствуют. Эта граница
доказывает точный controller-observed byte stream, а не непрерывность inode
между host и Docker daemon. Raw daemon observation входит в capability, но сам
daemon остаётся явно доверенным input объявленной границы.

Успешный replay хранится одним token-closed
`ContentResolvedEvaluatorReplayV1`, который повторно выводит три причинные
identity без зеркальных промежуточных dataclass:

1. source identity связывает lock, три admitted archive closures, build inputs
   и formula support. Job сюда не входит: одинаковый evaluator build не меняет
   source identity от конкретного RUN;
2. build identity связывает source identity, versioned Docker capability,
   pipeline policy, trust boundary, один sealed bundle object, два exact
   transfer и два byte-identical executable stdout. Comparator verifier
   заново выводит все десять preimage bytes и canonical manifest из того же
   operation snapshot и retained BUILD observation, затем сверяет все поля и
   identity с build observation. Это не независимая реализация или semantic
   replay, а exact re-derivation тех же причинных координат;
3. run identity впервые связывает canonical job с тем же retained executable
   bytes object, exact argv/env/cwd/stdin/limits, единственной допустимой
   `linux-x86_64` sandbox platform, typed child exit, stdout, canonical
   transcript и `RunClaimV1`.

Только one-shot controller может собрать этот корень, создать raw
`EvaluatorProvenanceClaimV1` и privately sealed
`SourceBoundEvaluatorReceiptV1`. Отдельного pipeline RUN-authority и public
promotion пути нет.

Ни raw claim, ни digest/content resolver, ни diagnostic BUILD/RUN object, ни
public constructor не создают receipt. Receipt доказывает только наблюдённую
причинность source → build → exact executable → run → stdout/transcript в
объявленной границе доверия. Он допустим для canonical transcript с
`BoundaryUnproven` или `ResourceLimitReached`. Semantic correctness, Arb/MPFI
diversity, mathematical proof и `DualProofReceiptV1` этим типом не представлены.
Host/Docker, instruction-level inputs, publisher origin и distribution
compliance не усиливаются и не называются SLSA/in-toto attestation.

## `ComparatorManifestV2`

Wire после `LCMAN2\0\0` содержит comparator kind `u8`
(`1 = Arb`, `2 = MPFI`), затем десять digest coordinates в фиксированном
порядке:

1. engine release;
2. upstream source;
3. arithmetic input set;
4. wrapper source;
5. evaluator source;
6. build identity, включая compiler, target и exact flags;
7. operation allowlist;
8. test observation;
9. legal file set;
10. exclusions.

Результат wire parse — только raw `ComparatorManifestV2`: его ненулевые
coordinates являются заявленными content addresses, а не доказанным
source binding. `ContentResolvedComparatorManifestV2` создаётся только
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
доказывает разное происхождение или реализацию. Arb receipt уже связывает свой
dependency graph; cross-path GMP/MPFR overlap и обязательные distinct edges не
считаются установленными без отдельного MPFI receipt.

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

`trace_digest` и `enclosure_digest` — только ненулевые content
coordinates, а не доказательство replay или enclosure. Semantic verification
требует разрешить и replay эти records, проверить exact equality/enclosure math
и связать результат с job, comparator, run и transcript; такого semantic
receipt в текущем release нет. Ни Arb-, ни MPFI-controller этого не делает. Любая
недоказанная transcendental equality
остаётся `BoundaryUnproven`: epsilon или midpoint не превращают её в
`Inside`.

`DecisionTranscriptV1` остаётся structural claim. Нулевые unresolved
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
цепь устанавливает только `SourceBoundArbControllerV1`; raw claim сам этого
права не имеет.

## `EvaluatorProvenanceClaimV1`

Wire после `LCPRV1\0\0` содержит три unresolved digest declarations:

1. provenance policy identity — versioned правила и trust boundary replay;
2. `RunClaimV1` identity — subject, к которому относится заявление;
3. replay evidence identity — unresolved coordinate source/build/run predicate.

Этот тип аналогичен структурному statement, а не attestation о выполненном
build. `parse` проверяет только canonical wire и ненулевые coordinates. Сам raw
тип не имеет public admission, resolver или флага успешного replay. Первый
sealed `SourceBoundEvaluatorReceiptV1` создаёт только Arb controller после
фактически наблюдённого typed replay DAG. Receipt identity равна identity
связанного claim и не дублирует subject отдельным digest; parse raw claim этого
права не даёт.

Назначение трёх внутренних coordinates только вдохновлено разделением ролей в
[in-toto Statement V1.2.0](https://github.com/in-toto/attestation/blob/v1.2.0/spec/v1/statement.md)
и definition/run model в
[SLSA Build Provenance V1.2](https://slsa.dev/spec/v1.2/build-provenance).
Wire остаётся внутренним бинарным протоколом Lab Colors и не заявляет
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
`ContentResolvedComparatorManifestV2`, согласованных `RunClaimV1` и
structurally admitted transcripts. Все bindings ведут к одному job, definition,
domain и policy; `domain_point_count` равен count связанного manifest.
Unresolved counters равны нулю, decision payloads совпадают побайтно,
а canonical raw equality witness bodies совпадают побайтно.

`BoundaryUnproven`, `ResourceLimitReached`, disagreement, foreign binding,
shared required-distinct coordinate или structurally missing equality witness возвращают typed
failure и не создают candidate. Успешный candidate фиксирует только
структурное согласие над exact bound domain manifest; он не является proof
receipt и не доказывает correctness ни одного evaluator.

`DualProofReceiptV1` требует semantic verification receipts для обоих evaluator
paths и независимой проверки их bindings/replay; этих admitted типов текущий
release не содержит. Family mint
дополнительно разрешает `domain_identity` и допускает отдельно exact full
manifest: единственный range `[0, 2^24)` и point count `2^24`. Совпадение
только point count или reduced-domain candidate этот gate не проходят.

## Ошибки допуска

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
