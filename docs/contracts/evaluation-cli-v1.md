# Внешний интерфейс оценки точки, версия 1

Успешный ответ содержит ровно следующие поля:

| Поле | Тип и смысл |
|---|---|
| `formatVersion`, `kind`, `ok` | Число `1`, строка `labcolors-declared-point-report-v1`, `true` |
| `scope`, `admission`, `human`, `rendererProvenance` | Только `modeled-srgb8-point`, `declared-package-policy-candidate`, `not-requested`, `unverified` |
| `terminalSrgb8`, `revision` | Массив ровно трёх целых `0..255`, целая текущая ревизия `u64` |
| `programIdentitySha256`, `profileIdentitySha256`, `conventionReleaseSha256`, `subjectIdentitySha256` | По 64 строчных hex-символа, полученных от действующих владельцев |
| `certificateHex` | Строчная чётная hex-строка единственного LCEN, без JSON-переинтерпретации нагрузки |

Потребитель принимает отчёт только после exit 0 и полного разбора единственного
документа с указанными kind/version и всеми обязательными полями. stdout до
завершения процесса или вывод `--help` не подтверждают оценку. При любом другом
исходе частичный результат отбрасывается.

Ошибка содержит ровно `formatVersion:1`, `ok:false`, `error:{domain,code}`.
Оба поля причины — фиксированные строки, без paths, произвольного input или
сообщения исключения. Ошибка вывода ошибки сама возвращает I/O exit 5.
Новый контракт ошибок:

| Exit / domain | Коды |
|---|---|
| 2 / `input` | `invalid_arguments`, `duplicate_format`, `missing_format`, `unsupported_format`, `unknown_option`, `too_many_inputs`, `invalid_jsonl_record`, `invalid_document`, `invalid_convention_digest`, `invalid_hex`, `missing_scenarios`, `invalid_program_wire` |
| 3 / `unsupported` | `unsupported_request`, `unsupported_scope`, `unsupported_admission`, `unsupported_human_evidence`, `family_artifacts_required`, `non_terminal_target`, `attachment_binding_rejected` |
| 3 / `clean-convention` или `technical-quality` | `unsupported_convention_release`, `unsupported_scope`, `unsupported_admission`, `unsupported_physical_identity`; только соответствующая ветвь владельца |
| 4 / `evaluation` | `program_compile_rejected`, `program_runtime_rejected`, `attachment_rejected`, `observation_rejected` |
| 4 / `clean-convention` | `rejected_by_convention`, `selection_required` |
| 4 / `technical-quality` или `clean-convention` | Точное имя отказа материализации: `materialization_not_ready`, `source_or_intermediate_paint`, `stale_revision`, `stale_identity`, `stale_sink_stamp`, `foreign_binding_epoch`, `terminal_binding_mismatch`, `missing_point_absence_proof`, `ambiguous_observation_cases` |
| 4 / `certificate` | Закрытый существующий `PointCertificateErrorV1::code`, включая недоступную identity производителя; ресурсный код относится к exit 6 |
| 5 / `io` | `read_failed`, `write_failed` |
| 6 / `resource` | `input_too_large`, `wire_too_large`, `allocation_refused`, `output_too_large`, `report_unavailable`, `attachment_resource_exhausted`, `observation_resource_exhausted`, `internal_invariant` |
| 6 / `evaluation`, `technical-quality` или `clean-convention` | `authority_inconsistent`: внутренний инвариант не подменяется отказом пользователя |
| 6 / `certificate` | Прежний `resource_limit_exceeded` |

Порядок отказов: аргументы → чтение/размер → JSONL/схема → версия и профильные
строки → длина/hex → сценарии → compile/attach/update → существующая цепочка
оценки и сертификат → ограниченная сериализация → внешняя запись. Недопустимая
кардинальность в JSON — `invalid_document`, а байтовый предел чтения —
`input_too_large`. Core сохраняет собственный порядок отказов внутри своей области.

Проверяемый исходный пример —
`crates/labcolors-evaluate-cli/examples/declared-point.json`. Его ProgramWire
получается **существующим** `ProgramWireBuilderV1`, а точная hex-строка затем
фиксируется в примере и сверяется с тем же каноническим сериализатором в тесте.
Граф: source `[128,128,129]`, opacity `0.5`, один surface input и source-over
occurrence; фон `[128,128,127]`, объявленное ограничение final `[128,128,128]`.
Выбран существующий выпуск конвенции `67cadaae38bbaea3096dba69142b5bf3d7776b7574ec224022abbcd119c45ce6`.
Этот пример проверяет подготовку входа, не заменяет независимый oracle цветовой
математики. Одна команда `cargo run --quiet --locked -p labcolors-evaluate-cli --
crates/labcolors-evaluate-cli/examples/declared-point.json` должна вернуть
exit 0, пустой stderr, итог `[128,128,128]` и LCEN. Контрастный пример
`rejected-point.json`: белый source, opacity `0.5`, фон `[1,1,3]`, final
`[128,128,129]`; он возвращает exit 4 / `clean-convention` / `rejected_by_convention`
и пустой stdout. Встроенного меню этих примеров в CLI нет.
