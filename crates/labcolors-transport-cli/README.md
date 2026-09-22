# labcolors-transport

CLI для ограниченного транспорта `CertificateEnvelopeV1`. Он не вычисляет цвет,
не принимает authority-решения и не создаёт producer capability. Семантика wire и
все отказы принадлежат `labcolors-core`; CLI владеет только вводом-выводом,
ограничением размера, стабильным JSON/JSONL и кодами завершения.

```text
labcolors-transport parse [--format json|jsonl] [FILE|-]
labcolors-transport serialize [--format json|jsonl] [FILE|-]
labcolors-transport inspect [--format json|jsonl] [FILE|-]
```

`FILE` по умолчанию означает stdin. За один запуск обрабатывается ровно один
bounded envelope. `parse` проверяет бинарный `LCEN` через Core и выдаёт lossless
transport-document v1 с lowercase `wireHex`. `serialize` принимает этот документ,
повторно проверяет восстановленные bytes через Core и пишет те же canonical bytes.
Нового wire-encoder в CLI нет. `inspect` выдаёт только публичную metadata-проекцию;
payload, capability и admission state в неё не попадают.

JSON использует pretty-форму с LF в конце. JSONL содержит тот же объект одной
компактной строкой с LF. Формат transport document:

```json
{
  "formatVersion": 1,
  "kind": "labcolors-certificate-envelope-v1",
  "wireHex": "..."
}
```

`wireHex` — непрозрачный транспорт canonical bytes, не authority result и не
доказательство научной истины. Неизвестные поля документа, иная версия/kind,
uppercase/non-hex spelling и envelope, который Core отвергает, fail closed.

`inspect` возвращает `formatVersion: 1`, kind
`labcolors-certificate-envelope-inspection-v1` и объект `certificate` с теми же
публичными полями, что decode-проекция WASM: schema/operation/authority tuple,
runtime artifact, producer revision/content identity, context, payload type/version,
payload length и два digest. Тело payload не выводится.

Ошибки всегда пишутся в stderr как одна компактная JSON-строка:
`{"formatVersion":1,"ok":false,"error":{"domain":"…","code":"…"}}`.
Коды завершения стабильны: `0` success, `2` transport/arguments, `3` typed
certificate refusal, `4` I/O, `5` внутренний resource/projection failure. Текст пути,
входные bytes, tuple и digest не включаются в ошибки.
