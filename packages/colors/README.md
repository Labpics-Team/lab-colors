# @labpics/colors

Компилятор и runtime проверяемых цветовых Program-графов.

Клиент передаёт канонические Program wire-байты. Ядро атомарно проверяет граф, создаёт Session, принимает наблюдения среды и возвращает только сертифицированные Paint outputs. Recipe-меню ролей и специальные Material/Glow runtime-пути удалены из публичного API.

## Установка

```sh
npm install @labpics/colors
```

## Первый маршрут

Инициализируйте WASM один раз выбранным способом, затем выполните общий пример
ниже в том же модуле.

### Браузер

В ESM-проекте с настройкой разрешения npm-импортов:

```ts
import init from "@labpics/colors";

await init();
```

Сборщик или сервер должен публиковать `pkg/labcolors_bg.wasm` рядом с
сгенерированным `pkg/labcolors.js`; `init()` загружает файл относительно этого
модуля. Если сборщик переносит WASM, передайте его URL явно через
`init({ module_or_path: wasmUrl })`.

### Node.js

В Node.js 22.11 или новее используйте ESM (`.mjs` или `"type": "module"` в
`package.json`) и передайте байты файла. Node.js не загружает `file:` URL через
`fetch`, поэтому браузерный вызов без аргументов здесь не подходит.

```ts
import { readFileSync } from "node:fs";
import init from "@labpics/colors";

await init({
  module_or_path: readFileSync(
    new URL(import.meta.resolve("@labpics/colors/pkg/labcolors_bg.wasm")),
  ),
});
```

### Объявление графа и чтение результата

```ts
import { compileProgramWire } from "@labpics/colors";
import {
  ProgramWireBuilderV1,
  SURROUND_AVERAGE_V1,
  WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1,
} from "@labpics/colors/program-wire/abi-v1.js";

const programBytes = new ProgramWireBuilderV1()
  .source(11, [20, 20, 20])
  .fixedTarget(21, 11)
  .surfaceInputPort(31)
  .solidPaint(41, 21)
  .inputSurface(51, 31)
  .sourceOverOccurrence(61, 41, 51, 64, 0.2, SURROUND_AVERAGE_V1)
  .presentationRoot(71, 61)
  .presentationTarget(71, 61)
  .wcag22VisibleUnary(true, 81, 61, WCAG22_SC1411_UI_COMPONENT_OR_STATE_V1)
  .output(91, 41)
  .finish();
const runtime = compileProgramWire(programBytes, 1);
try {
  const snapshot = runtime.updateObserved(
    1n,
    new Uint32Array([1]),
    new Uint8Array([255, 255, 255]),
    1,
  );
  try {
    if (snapshot.state === "ready") {
      for (let index = 0; index < snapshot.outputCount(); index += 1) {
        console.log(
          snapshot.outputSlot(index),
          snapshot.outputRgb(index),
          snapshot.outputOpacity(index),
        );
      }
    }
  } finally {
    snapshot.free();
  }
} finally {
  runtime.free();
}
```

Канонические `LCPW` v1 bytes создаются `ProgramWireBuilderV1` или другим реализационно-независимым энкодером того же контракта.

## Явная конвенция cleanliness

Чтобы выбранный результат удовлетворял встроенной конвенции sRGB8, добавьте
`.declaredSrgb8CleanSet(true, 82, 71, 61)` в цепочку примера перед `.finish()`.
Здесь `82` — ID ограничения, `71` — presentation root, `61` — occurrence.
Проверяется финальный encoded-sRGB8 результат этой точки с учётом композиции,
а не только исходный цвет. `true` задаёт обязательное ограничение; `false`
не ограничивает выдачу outputs. Результаты отдельных report-only ограничений
текущий публичный `ProgramSnapshot` не раскрывает.

Это явно выбранная, закреплённая версией пакета конвенция. Она не включается
автоматически и не подтверждает человеческое восприятие, фактическую отрисовку
или универсальную «чистоту» цвета.

## Подключение к внешнему sink

Для сценария, где приложение само владеет DOM или другим renderer-surface,
используйте `attachProgramWire`. Синхронный `host` получает полный typed intent
(`setAll`, `revokeAll` или `confirmExact`) и должен вернуть `true` только после
атомарной установки всего принадлежащего ему scope; `false` отклоняет update.
Пакет не пишет DOM и не выдаёт authority из исходного или промежуточного Paint.
В примере `applyOwnedPointIntent` и `revokeOwnedPointScope` реализует приложение:
первая функция атомарно применяет intent и возвращает `boolean`, вторая
возвращает управление только после отзыва всего принадлежащего sink scope.

```ts
import { attachProgramWire } from "@labpics/colors";

const attachment = attachProgramWire(programBytes, 1, 91, 501, 71, 61, (intent) => {
  // Приложение проверяет sinkOutput, expectedSequence и bindingEpoch,
  // затем одним действием обновляет собственный renderer-surface.
  return applyOwnedPointIntent(intent);
});
try {
  const snapshot = attachment.updateObserved(
    1n,
    new Uint32Array([1]),
    new Uint8Array([255, 255, 255]),
    1,
  );
  try {
    if (snapshot.state === "ready") {
      const authority = attachment.materializationAuthority();
      try {
        console.log(authority.terminalCompositeRgb());
      } finally {
        authority.free();
      }
    }
  } finally {
    snapshot.free();
  }
} finally {
  revokeOwnedPointScope();
  attachment.dispose(true);
  attachment.free();
}
```

`materializationAuthority()` читает только текущий успешно установленный
attachment head. Его `rendererProvenance` остаётся `"unverified"`: результат —
модельная terminal materialization с проверенными identity, revision, sink stamp,
binding epoch и отсутствием downstream point, а не доказательство browser paint
или человеческого восприятия. При отказе host предыдущий head сохраняется.
Пока host callback исполняется синхронно, повторный вызов любой операции attachment,
включая `free()`, получает typed-отказ busy; освобождайте attachment после возврата
из callback. `free()` также получает typed-отказ `program_attachment_revoke_unconfirmed`,
пока внешний scope не отозван и `dispose(true)` не завершился успешно. Внешний scope
отзывается владельцем host до `dispose(true)`. Подтверждением служит только
примитивный `true`; числа, строки и объекты не приводятся к boolean и получают
`program_attachment_revoke_unconfirmed`.

`Symbol.dispose` и TypeScript `using` выполняют тот же защищённый `free()`.
До выхода из `using` приложение должно отозвать scope и вызвать `dispose(true)`;
автоматическое освобождение не подтверждает отзыв за внешнего владельца.

## Создание и инспекция certificate envelope

После инициализации WASM `issueSourceCertificateEnvelope()` возвращает новый
`Uint8Array` с envelope встроенного source producer Core. Функция не принимает
payload или identity. Например, после `await init()`:

```ts
const bytes = issueSourceCertificateEnvelope();
const metadata = decodeCertificateEnvelope(bytes);
```

Обе функции импортируются из `@labpics/colors`. `metadata.producerRevision` —
полный Git tree object ID исходников Core, а content identity адресует встроенный
source descriptor. Эти данные не утверждают равенство исполняемых файлов,
аутентичность удалённого отправителя или научные свойства цвета.

Если при сборке проверенный source descriptor недоступен, issuance бросает
ошибку с `operation: "issueSourceCertificateEnvelope"` и
`code: "certificate_producer_identity_unavailable"`. Её распознаёт
`isCertificateError`; untrusted decode и остальные операции остаются доступны.

`decodeCertificateEnvelope(bytes)` разбирает только фиксированный `LCEN` v1
transport envelope и возвращает замороженный metadata-object
`UntrustedCertificateEnvelopeV1`. Результат — метаданные framing и проверенные digest,
а не authority result и не доказательство
истины payload. Тело payload остаётся opaque и не передаётся в JavaScript.

До вызова WASM-функции фасад проверяет intrinsic storage `Uint8Array`, безопасно
нормализует честный subclass (включая Node `Buffer`) и отвергает Proxy либо
подменённые `length`/`byteLength`; размер больше `MAX_CERTIFICATE_ENVELOPE_BYTES`
(`2097152`) отвергается до копирования, поскольку `wasm-bindgen` копирует typed array
в linear memory до входа Rust. Неизвестная schema, authority,
operation, payload type/version, неканоничные строки, trailing bytes и digest
mismatch дают typed error; `isCertificateError(value)` проверяет только
допустимую пару `operation`/`code`.

Заморожен внешний metadata-object; его три byte-поля — отдельные detached snapshots,
поэтому изменение буфера результата не меняет состояние WASM или следующий decode.

Создать envelope из произвольных JS-байтов, `ProgramSnapshot`, Paint, CSS, DOM или
materialization authority нельзя. Producer capability и admission ledger остаются
владением Rust Core; в r13 нет persistence, network relay, CLI, science authority
или registry publication.

## Контракт

- Один публичный runtime-root: `compileProgramWire` → `ProgramRuntime` → `ProgramSnapshot`.
- Обновление атомарно: отказ не публикует частичный state.
- Output появляется только вместе с сертификатом полного hard-support. Это внутренняя проверка в рамках Session, а не публичный переносимый сертификат или гарантия конечного видимого результата приложения.
- Канонические байты имеют одну `ContentIdentity`.
- Невалидные wire-байты, графы, observation и resource bounds возвращают typed-отказы; fallback отсутствует.
- DOM и CSS не входят в ядро. Применение output принадлежит приложению.

## Числовые входы

ID потока и причины, число подложек и индекс output принимаются только как
конечные целые `number` в диапазоне `0..4294967295`. Ревизия принимается как
`bigint` в диапазоне `0n..18446744073709551615n`. Строки, нечисловые значения,
дроби и выход за диапазон не приводятся к допустимому значению. Допуск входа
предшествует изменению Session; после отказа корректную ревизию можно повторить.
Допустимость конкретного индекса, формы observations и порядка ревизий
по-прежнему проверяет соответствующий контракт runtime.

## Распознавание ошибок

`isProgramError(value)` распознаёт ошибки Program по принадлежности текущему
`Error` и допустимой паре `operation`/`code`. Неизвестное значение, чужая пара
или исключение при чтении возвращают `false`, не заменяя исходную ошибку
новым исключением. Каждое поле читается не более одного раза, без преобразования
его значения; читаемые унаследованные поля и getters допустимы.

Нехватка ресурсов при compile/instantiate/update возвращает
`program_resource_exhausted`, внутренний сбой — `program_internal_invariant`.
Attachment сохраняет те же классы в `program_attachment_resource_exhausted`
и `program_attachment_internal_invariant`. Эти причины не означают, что
пользовательский граф или observation семантически недопустимы; отказ update
сохраняет предыдущий committed head и допускает повтор той же ревизии.

Проверка описывает только наблюдённые значения. Она не удостоверяет происхождение
ошибки, не замораживает объект и не гарантирует неизменность следующих чтений
пользовательских getters. Непознанная ошибка остаётся у вызывающего приложения.

## Дополнительные функции

- `evaluateWcag22` — точная оценка WCAG 2.2 в finite sRGB8/Q55-профиле.
- `numericalCapabilityManifest` — манифест численных возможностей и доказательств сборки.

Публикация registry/deploy не является частью этого изменения: пакет проверяется из точного CI tarball до отдельного release-гейта.
