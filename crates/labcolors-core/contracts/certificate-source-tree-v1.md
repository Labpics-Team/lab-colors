# Source producer certificate v1

Этот профиль создаёт non-semantic LCEN v1 envelope для исходного артефакта
`Labpics-Team/lab-colors:crates/labcolors-core`. Идентичность включает выбранное
Git-дерево этого каталога; она не охватывает все входы компилятора и не утверждает
равенство бинарных файлов, научную корректность или remote authentication.

## Descriptor и tuple

Пусть `T` — полный 40-символьный lowercase SHA-1 ID объекта типа `tree`,
выбранного как `HEAD:crates/labcolors-core`. Это tree revision, не commit ID.
Descriptor имеет ровно 47 bytes:

`ASCII("LCST") || u16_be(1) || u8(1) || ASCII(T)`.

Последний selector `1` обозначает GitSha1Tree. Пусть
`D = SHA256(ASCII("labpics.colors/core-source-tree-descriptor/v1") || [0x00] || descriptor)`.

| Поле LCEN v1 | Каноническое значение |
|---|---|
| operation | IssueCertificate, 0x01 |
| authority kind/version | GenericTypedCertificate, 0x00 / 1 |
| producer revision | T |
| producer content identity | 32 raw bytes D |
| runtime artifact ID | `labcolors-core:source-tree-v1:` + lowercase hex D, 94 bytes |
| context ID | `core-source-tree-transport-v1`, 29 bytes |
| payload type/version | NonSemanticTransportPayloadV1 / 1 |
| payload | Один producer-owned byte 0x01 |

Schema, positional framing, digest domains и limits принадлежат LCEN v1.
Envelope этого профиля имеет 283 bytes. Marker не несёт authority result.
Budget/inventory вне Core не входят в T; tests/docs внутри Core входят.

## Проверка источника при сборке

Build owner получает T из Git без поиска по истории и проверяет точный crate path,
object format и kind. Short/uppercase ID, Git SHA-256 namespace, missing objects
или недоступный источник не заменяются environment/package version.

Фактический набор путей должен совпасть с Git-деревом. Для regular entries
100644/100755 сравниваются raw file bytes и regular-file kind; physical executable
bit не является частью этой реализации исходников. Git mode остаётся частью T.
Для entry 120000 сравниваются symlink kind и буквальные bytes цели ссылки,
прочитанные без following, normalization или хеширования target-файла. Такое
правило применяется ко всем ссылкам, включая Core/LICENSE. Gitlink/special kinds,
missing/extra entries, изменение blob bytes или literal symlink target дают отказ.
Clean/smudge normalization и index flags не заменяют сравнение actual bytes.

Выбранное дерево проверяется до и после наблюдения. Сборка выполняется в
изолированном checkout без concurrent writer; это предпосылка trusted build,
не защита от злонамеренного compiler. Git/filesystem доступны только build owner.
Runtime использует immutable generated descriptor и не выполняет I/O.

## Cache и unavailable

Cargo наблюдает каталог crate и разрешённые Git metadata для обычного checkout
и linked worktree: HEAD, index, symbolic/common refs, packed-refs/config и путь
появления Git при его отсутствии. Пока обязательный metadata input отсутствует,
наблюдается также его ближайший существующий parent; после восстановления этот
дополнительный watch удаляется. Для valid identity generated output расположен
вне Core subtree. Обычный target внутри standalone archive допускает сборку,
но делает producer identity unavailable, исключая generated self-input.
При отказе прежний valid descriptor заменяется unavailable, а не сохраняется.

Descriptor описывает исходный артефакт скомпилированного runtime, не текущее
состояние прав файлов. Обычный chmod executable bit без Git commit может оставить
тот же compiled descriptor; возврат бита также не меняет identity. Committed mode
или symlink-target change меняет T и требует проверки нового source artifact.
Произвольная подмена cache/compiler или сохранённых timestamps не входит в claim.

Archive без Git, непроверенный или изменившийся source, unsupported observation
дают `CertificateProducerErrorV1::ProducerIdentityUnavailable`. В generated
`certificate-source-tree-v1.bin` это empty file; valid state — ровно 47 bytes.
Malformed generated descriptor тоже отвергается. Остальные Core APIs и untrusted
decode остаются доступны. Отказ не создаёт placeholder revision или capability.

## Выдача и admission

`certificate::issue_source_certificate_v1()` не принимает identity/payload.
Он создаёт sealed marker и matching non-serializable attestation, затем вызывает
`CertificateEnvelopeV1::issue_from_trusted_producer`. Public result хранит private
fields; Rust consumer заимствует capability через `attestation()` и передаёт
свой полный expected `AdmissionKeyV1` в существующий `AdmissionStateV1::admit`.

WASM `issueSourceCertificateEnvelope()` возвращает отдельный byte buffer.
Decode этих bytes остаётся untrusted inspection; JS не получает capability или
admit API. Wire/admission errors сохраняются; producer unavailable использует
static `certificate_producer_identity_unavailable` с operation
`issueSourceCertificateEnvelope`. Debug/errors не содержат T, D, path или payload.
